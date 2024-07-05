use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use crate::background::procedure::BackgroundProcedure;

pub struct AudioCtrls {
    pub player: player::StreamControl,
    pub ecouter: BackgroundProcedure<Vec<f32>, ecouter::StreamControlCommand>,
    pub db: Arc<Mutex<database::FSDatabase>>,
}

pub fn setup() -> anyhow::Result<AudioCtrls> {
    let db = Arc::new(Mutex::new(database::FSDatabase::new()));
    let host = cpal::default_host();
    let ectrl = ecouter::setup(&host)?;
    let pctrl = player::setup(&host, db.clone())?;

    return Ok(AudioCtrls {
        player: pctrl,
        ecouter: ectrl,
        db,
    });
}

pub mod player {
    use std::{
        fs,
        io::BufReader,
        sync::{
            mpsc::{channel, Sender},
            Arc, Mutex,
        },
        time::Duration,
    };

    use anyhow::{anyhow, Context};
    use cpal::traits::{DeviceTrait, HostTrait};

    use rodio::Source;

    pub enum StreamControlCommand {
        /// play audio item by id
        Play(String),
        Pause(String),
    }

    pub struct StreamControl {
        tx: Sender<StreamControlCommand>,
    }

    impl StreamControl {
        pub fn start(&self, id: String) {
            self.tx
                .send(StreamControlCommand::Play(id))
                .expect("failed to send Control::Play through channel");
        }

        pub fn pause(&self, id: String) {
            self.tx
                .send(StreamControlCommand::Pause(id))
                .expect("failed to send Control::Pause through channel");
        }
    }

    use crate::audio::database::UpdateParams;

    use super::{app_dir, database::FSDatabase};

    pub fn setup(host: &cpal::Host, db: Arc<Mutex<FSDatabase>>) -> anyhow::Result<StreamControl> {
        let speakers = host
            .default_output_device()
            .expect("no input device available");

        let supported_config = speakers
            .default_output_config()
            .expect("no supported ouput config?!");

        let config: cpal::StreamConfig = supported_config.into();
        eprintln!("[debug] output config: {:?}", config);

        let (send_ctrl, receiver_ctrl) = channel::<StreamControlCommand>();

        // Thread that handles play/pause commands
        let _handle = std::thread::spawn(move || {
            let mut current_audio_item_id: Option<String> = None;

            let (_stream, handle) = rodio::OutputStream::try_default().unwrap();
            let sink = Arc::new(rodio::Sink::try_new(&handle).unwrap());
            let sink_ = Arc::clone(&sink);

            let (tx_id, rx_id) = channel::<String>();

            let audio_duration = Arc::new(Mutex::new(Duration::ZERO));
            let audio_duration_ref = Arc::clone(&audio_duration);
            let _ = std::thread::spawn(move || loop {
                let id = rx_id.recv().unwrap();
                let wavfilepath = app_dir().join(&id).with_extension("wav");

                let file = fs::File::open(&wavfilepath)
                    .context(anyhow!("failed to open file: {:?}", wavfilepath))
                    .expect("failed to open wav file for reading");

                sink_.stop();

                eprintln!("[info] audio item {} is playing", id);

                // let file = std::fs::File::open("/home/gnarus/d/voechol/src-tauri/target/debug/build/whisper-rs-sys-44ecd8d3e9c7f53a/out/whisper.cpp/samples/jfk.mp3").unwrap();
                let dec = rodio::Decoder::new(BufReader::new(file)).unwrap();

                if let Some(dur) = dec.total_duration() {
                    *audio_duration_ref.lock().unwrap() = dur;
                    eprintln!("[trace] audio duration: {:?}", dur);
                }

                sink_.append(dec);

                sink_.play();
            });

            eprintln!("[info] player is ready");
            loop {
                let ctrl = receiver_ctrl.try_recv();

                match ctrl {
                    Ok(StreamControlCommand::Play(id)) => {
                        eprintln!("[info] requested to play item: {}", id);

                        db.lock()
                            .unwrap()
                            .update_audio_items(UpdateParams {
                                id: &id,
                                is_playing: Some(true),
                                filepath: None,
                            })
                            .expect("failed to mark audio item as playing");

                        if current_audio_item_id.as_ref() != Some(&id) {
                            current_audio_item_id = Some(id.clone());
                            tx_id.send(id).expect("failed to send audio item id");
                        }

                        sink.play();
                    }
                    Ok(StreamControlCommand::Pause(id)) => {
                        eprintln!("[info] requested to pause item: {}", id);

                        sink.pause();

                        db.lock()
                            .unwrap()
                            .update_audio_items(UpdateParams {
                                id: &id,
                                is_playing: Some(false),
                                filepath: None,
                            })
                            .expect("failed to mark audio item as paused");
                    }
                    Err(err) => match err {
                        std::sync::mpsc::TryRecvError::Empty => {
                            if let Some(id) = current_audio_item_id.as_ref() {
                                let dur = *audio_duration.lock().unwrap();
                                // eprintln!("[trace] audio pos {:?} vs dur {:?}", sink.get_pos(), dur);
                                if dur.as_secs() != 0 && sink.get_pos().as_secs() >= dur.as_secs() {
                                    // eprintln!("[info] audio item {} is done playing", id);
                                    db.lock()
                                        .unwrap()
                                        .update_audio_items(UpdateParams {
                                            id,
                                            is_playing: Some(false),
                                            filepath: None,
                                        })
                                        .expect("failed to mark audio item as playing");
                                }
                            }
                        }
                        std::sync::mpsc::TryRecvError::Disconnected => {
                            return;
                        }
                    },
                };
            }
        });

        Ok(StreamControl { tx: send_ctrl })
    }
}

pub mod ecouter {
    use std::sync::Arc;

    use cpal::traits::StreamTrait;

    use crate::{
        audio::{self, audio_stream_err_fn, database::wav_spec_from},
        background::procedure::BackgroundProcedure,
    };

    pub enum StreamControlCommand {
        Play,
        Pause,
    }

    pub fn setup(
        host: &cpal::Host,
    ) -> anyhow::Result<BackgroundProcedure<Vec<f32>, StreamControlCommand>> {
        use cpal::traits::{DeviceTrait, HostTrait};

        let mic = host
            .default_input_device()
            .expect("no input device available");

        let supported_config = mic
            .default_input_config()
            .expect("no supported input config?!");
        let config = supported_config.into();

        eprintln!("[debug] input config: {:?}", config);

        let job_handle =
            BackgroundProcedure::<Vec<f32>, StreamControlCommand>::setup(vec![], move |arg| {
                let audio_buffer = arg.state;
                let audio_buffer_ref = Arc::clone(&audio_buffer);

                let stream = mic
                    .build_input_stream(
                        &config,
                        move |data: &[f32], _| {
                            eprintln!("[info]: data len {:?}", &data.len());
                            audio_buffer_ref
                                .lock()
                                .expect("failed to lock on audio_buffer")
                                .extend(data);
                        },
                        audio_stream_err_fn,
                        None,
                    )
                    .expect("failed to build_input_stream");

                stream.pause().expect("failed to pause the input stream");

                let on_pause = |stream: &cpal::Stream, config: &cpal::StreamConfig| {
                    stream.pause().expect("failed to pause the input stream");
                    eprintln!("[info] done listening");

                    let mut db = audio::database::FSDatabase::new();

                    let audio_item = audio::AudioItem::new(
                        &audio_buffer.lock().expect("failed to lock on audio_buffer"),
                    );

                    eprintln!("[info] write wav file for new audio item");
                    {
                        db.write_to_wav(
                            &audio_buffer.lock().expect("failed to lock on audio_buffer"),
                            &audio_item.id,
                            wav_spec_from(config),
                        );
                    }

                    eprintln!("[info] saving audio item");
                    db.save_audio_item(audio_item)
                        .expect("failed to to save new audio item");

                    audio_buffer
                        .lock()
                        .expect("failed to lock on audio_buffer")
                        .clear();
                    eprintln!("[trace] cleared audio_buffer");
                };

                loop {
                    let ctrl = arg
                        .rx
                        .recv()
                        .expect("failed to receive from control channel");

                    match ctrl {
                        StreamControlCommand::Play => {
                            eprintln!("[info] listening...");
                            stream.play().expect("failed to play the input stream");
                        }
                        StreamControlCommand::Pause => on_pause(&stream, &config),
                    };
                }
            });

        Ok(job_handle)
    }
}

mod database {
    use std::{
        fs,
        path::{Path, PathBuf},
    };

    use anyhow::Context;

    use crate::audio::app_dir;

    use super::{audio_items_data_file, AudioItem};

    pub struct FSDatabase {
        wav_dir: PathBuf,
        datafile: PathBuf,
    }

    pub struct UpdateParams<'i> {
        pub id: &'i str,
        pub filepath: Option<&'i Path>,
        pub is_playing: Option<bool>,
    }

    pub fn wav_spec_from(config: &cpal::StreamConfig) -> hound::WavSpec {
        hound::WavSpec {
            channels: config.channels,
            sample_rate: config.sample_rate.0,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        }
    }

    impl FSDatabase {
        pub fn new() -> Self {
            Self {
                wav_dir: app_dir(),
                datafile: audio_items_data_file(),
            }
        }

        pub fn items(&self) -> Vec<AudioItem> {
            Self::load_all().unwrap()
        }

        pub fn write_to_wav(&mut self, buffer: &[f32], id: &str, spec: hound::WavSpec) {
            eprintln!("[info] writing wav with specs: {:?}", spec);
            let wav_filepath = self.wav_dir.join(id).with_extension("wav");
            let mut writer =
                hound::WavWriter::create(wav_filepath, spec).expect("failed to create wav writer");

            for sample in buffer.iter() {
                writer
                    .write_sample(*sample * 2.0)
                    .expect("failed to write sample");
            }
        }

        pub fn load_all() -> anyhow::Result<Vec<AudioItem>> {
            let Ok(data) = fs::read_to_string(audio_items_data_file())
                .context("failed to read from audio items data file")
            else {
                return Ok(vec![]);
            };

            let items: Vec<_> = serde_json::from_str(&data)
                .context("failed to parse audio items data file json")?;

            return Ok(items);
        }

        pub fn save_audio_item(&mut self, item: AudioItem) -> anyhow::Result<()> {
            let mut items = self.items();
            items.push(item);

            self.save_all()
                .context("failed to save new audio item in data file")?;

            Ok(())
        }

        fn save_all(&self) -> anyhow::Result<()> {
            let json_string =
                serde_json::to_string(&self.items()).context("failed to Serialize audio item")?;

            fs::write(&self.datafile, json_string)?;

            Ok(())
        }

        pub fn update_audio_items(&mut self, params: UpdateParams) -> anyhow::Result<()> {
            for item in self.items().iter_mut() {
                if item.id == params.id {
                    item.is_playing = params.is_playing.unwrap_or(item.is_playing);
                    item.filepath = params
                        .filepath
                        .map(|p| p.to_path_buf())
                        .unwrap_or(item.filepath.clone());
                    self.save_all()?;
                    break;
                }
            }

            Ok(())
        }
    }
}

mod stt {
    use std::sync::atomic::AtomicBool;

    use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

    pub static IS_TRANSCRIBING: AtomicBool = AtomicBool::new(false);

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
            IS_TRANSCRIBING.store(true, std::sync::atomic::Ordering::Relaxed);

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

            IS_TRANSCRIBING.store(false, std::sync::atomic::Ordering::Relaxed);

            text
        }
    }

    /// Assuming mic input stream gave us the same sample for each of 2 channels.
    /// So we can discard one of the channels safely.
    pub fn stereo_to_mono(buffer: &[f32]) -> Vec<f32> {
        buffer.chunks(2).map(|c| c[0]).collect()
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AudioItem {
    pub id: String,
    pub excerpt: String,
    pub filepath: PathBuf,
    #[serde(default)]
    pub is_playing: bool,
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
            is_playing: false,
        }
    }
}

pub mod polling {
    use super::AudioItem;

    #[derive(serde::Serialize, Debug)]
    pub struct RecordingsPoll {
        is_transcribing: bool,
        audio_items: Vec<AudioItem>,
    }

    impl RecordingsPoll {
        pub fn poll(db: &super::database::FSDatabase) -> anyhow::Result<Self> {
            let is_transcribing =
                super::stt::IS_TRANSCRIBING.load(std::sync::atomic::Ordering::Relaxed);

            Ok(Self {
                audio_items: db.items().to_vec(),
                is_transcribing,
            })
        }
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

fn audio_stream_err_fn(err: cpal::StreamError) {
    eprintln!("[error] an error occurred on stream: {}", err);
}
