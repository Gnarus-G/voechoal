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
                        eprintln!("[info]: {:?}", data[0]);
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
    use std::{
        fs,
        path::{Path, PathBuf},
    };

    use cuid2::cuid;

    pub struct WavWriter {
        spec: hound::WavSpec,
        save_dirpath: PathBuf,
    }

    impl WavWriter {
        pub fn setup(config: &cpal::StreamConfig) -> Self {
            let home_dir = std::env::var("HOME").expect("failed to resolve $HOME variable");
            let save_dir = Path::new(&home_dir).join("voechoal");

            if !save_dir.is_dir() {
                fs::create_dir(&save_dir).expect("failed to create save dir");
            }

            let spec = hound::WavSpec {
                channels: config.channels,
                sample_rate: config.sample_rate.0,
                bits_per_sample: 32,
                sample_format: hound::SampleFormat::Float,
            };

            eprintln!("[info] wav specs: {:?}", spec);

            Self {
                spec,
                save_dirpath: save_dir,
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
