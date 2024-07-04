use std::{fs, path::PathBuf};

use anyhow::Context;

pub mod ecouter {
    use std::sync::{
        mpsc::{channel, Sender},
        Arc, Mutex,
    };

    use cpal::traits::StreamTrait;

    use crate::audio;

    enum Control {
        Play,
        Pause,
    }

    pub struct IsRecordingCtrl {
        tx: Sender<Control>,
    }

    impl IsRecordingCtrl {
        pub fn start(&self) {
            self.tx
                .send(Control::Play)
                .expect("failed to send Control::Play through channel");
        }

        pub fn pause(&self) {
            self.tx
                .send(Control::Pause)
                .expect("failed to send Control::Pause through channel");
        }
    }

    pub fn setup() -> Result<IsRecordingCtrl, String> {
        use cpal::traits::{DeviceTrait, HostTrait};

        let host = cpal::default_host();

        let mic = host
            .default_input_device()
            .expect("no input device available");

        let supported_config = mic.default_input_config().expect("no supported config?!");
        let config = supported_config.into();

        let (send_ctrl, receiver_ctrl) = channel::<Control>();

        let _handle = std::thread::spawn(move || {
            let audio_buffer = Arc::new(Mutex::new(vec![] as Vec<f32>));
            let audio_buffer_ref = Arc::clone(&audio_buffer);

            let stream = mic
                .build_input_stream(
                    &config,
                    move |data: &[f32], _| {
                        eprintln!("[info]: {:?}", &data[0..2]);
                        audio_buffer_ref
                            .lock()
                            .expect("failed to lock on audio_buffer")
                            .extend(data);
                    },
                    err_fn,
                    None,
                )
                .expect("failed to build_input_stream");

            stream.pause().expect("failed to pause the input stream");

            let on_pause = |stream: &cpal::Stream, config: &cpal::StreamConfig| {
                stream.pause().expect("failed to pause the input stream");

                let mut writer = audio::save::WavWriter::setup(config);

                {
                    let buffer = audio_buffer.lock().expect("failed to lock on audio_buffer");
                    writer.write_to_wav(&buffer);
                }

                let audio_item = audio::AudioItem::new(
                    &audio_buffer.lock().expect("failed to lock on audio_buffer"),
                );
                audio_item.save().expect("failed to to save new audio item");

                audio_buffer
                    .lock()
                    .expect("failed to lock on audio_buffer")
                    .clear();
            };

            loop {
                let ctrl = receiver_ctrl
                    .recv()
                    .expect("failed to receive from control channel");

                match ctrl {
                    Control::Play => stream.play().expect("failed to play the input stream"),
                    Control::Pause => on_pause(&stream, &config),
                };
            }
        });

        Ok(IsRecordingCtrl { tx: send_ctrl })
    }

    fn err_fn(err: cpal::StreamError) {
        eprintln!("[error] an error occurred on stream: {}", err);
    }
}

mod save {
    use std::path::PathBuf;

    use cuid2::cuid;

    use crate::audio::app_dir;

    pub struct WavWriter {
        spec: hound::WavSpec,
        save_dirpath: PathBuf,
    }

    impl WavWriter {
        pub fn setup(config: &cpal::StreamConfig) -> Self {
            let spec = hound::WavSpec {
                channels: config.channels,
                sample_rate: config.sample_rate.0,
                bits_per_sample: 32,
                sample_format: hound::SampleFormat::Float,
            };

            eprintln!("[info] wav specs: {:?}", spec);

            Self {
                spec,
                save_dirpath: app_dir(),
            }
        }

        pub fn write_to_wav(&mut self, buffer: &[f32]) {
            let id = cuid();
            let wav_filepath = self.save_dirpath.join(id).with_extension("wav");
            let mut writer = hound::WavWriter::create(wav_filepath, self.spec)
                .expect("failed to create wav writer");

            for sample in buffer.iter() {
                writer
                    .write_sample(*sample * 2.0)
                    .expect("failed to write sample");
            }
        }
    }
}

mod stt {
    use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

    pub struct Transcribe {
        ctx: WhisperContext,
    }

    impl Transcribe {
        pub fn new(path_to_model: &str) -> Self {
            let ctx =
                WhisperContext::new_with_params(path_to_model, WhisperContextParameters::default())
                    .expect("failed to load model");

            Self { ctx }
        }

        pub fn transcribe(&self, audio_data: &[f32], prompt: &str) -> String {
            let ctx = &self.ctx;

            let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });

            let tokens = &ctx.tokenize(prompt, prompt.len()).unwrap();
            params.set_tokens(tokens);

            params.set_n_threads(1);
            params.set_print_progress(false);
            params.set_print_realtime(false);
            params.set_print_timestamps(false);
            params.set_no_context(true);
            params.set_suppress_non_speech_tokens(true);

            // now we can run the model
            let mut state = ctx.create_state().expect("failed to create state");
            state.full(params, audio_data).expect("failed to run model");

            // fetch the results
            let num_segments = state
                .full_n_segments()
                .expect("failed to get number of segments");

            // average english word length is 5.1 characters which we round up to 6
            let mut text = String::with_capacity(6 * num_segments as usize);

            for i in 0..num_segments {
                let segment = state
                    .full_get_segment_text(i)
                    .expect("failed to get segment");

                text.push_str(&segment);
            }

            text
        }
    }

    /// Assuming mic input stream gave us the same sample for each of 2 channels.
    /// So we can discard one of the channels safely.
    pub fn stereo_to_mono(buffer: &[f32]) -> Vec<f32> {
        buffer.chunks(2).map(|c| c[0]).collect()
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct AudioItem {
    pub id: String,
    pub excerpt: String,
    pub filepath: PathBuf,
}

impl AudioItem {
    pub fn new(buffer: &[f32]) -> Self {
        let mono = stt::stereo_to_mono(buffer);

        let tt = stt::Transcribe::new("/home/gnarus/d/caldi/models/ggml-base.en.bin");

        let transcript = tt.transcribe(
            &mono,
            "[system]\nTranscribe the first 24 words in the song that the user is singing.\n[user]",
        );

        let id = cuid2::cuid();

        Self {
            filepath: app_dir().join(&id).with_extension("wav"),
            id,
            excerpt: transcript,
        }
    }

    pub fn load_all() -> anyhow::Result<Vec<Self>> {
        let Ok(data) = fs::read_to_string(audio_items_data_file())
            .context("failed to read from audio items data file")
        else {
            return Ok(vec![]);
        };

        let items: Vec<_> =
            serde_json::from_str(&data).context("failed to parse audio items data file json")?;

        return Ok(items);
    }

    pub fn save(self) -> anyhow::Result<()> {
        let mut items = Self::load_all()?;
        items.push(self);

        let json_string =
            serde_json::to_string(&items).context("failed to Serialize audio item")?;

        fs::write(audio_items_data_file(), json_string)
            .context("failed to save new audio item in data file")?;

        Ok(())
    }
}

fn app_dir() -> PathBuf {
    let home_dir = std::env::var("HOME").expect("failed to resolve $HOME variable");
    let save_dir = std::path::Path::new(&home_dir).join("voechoal");

    if !save_dir.is_dir() {
        std::fs::create_dir(&save_dir).expect("failed to create save dir");
    }

    return save_dir;
}

fn audio_items_data_file() -> PathBuf {
    app_dir().join("data").with_extension("json")
}
