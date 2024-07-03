pub mod ecouter {
    use std::sync::mpsc::{channel, Sender};

    use cpal::traits::StreamTrait;

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

        let mut supported_configs_range = mic
            .supported_input_configs()
            .expect("error while querying configs");

        let supported_config = supported_configs_range
            .next()
            .expect("no supported config?!")
            .with_max_sample_rate();

        let config = supported_config.into();

        let (send_ctrl, receiver_ctrl) = channel::<Control>();

        let _handle = std::thread::spawn(move || {
            let stream = mic
                .build_input_stream(
                    &config,
                    move |data: &[f32], _| {
                        eprintln!("[info]: {:?}", data[0]);
                    },
                    err_fn,
                    None,
                )
                .expect("failed to build_input_stream");

            eprintln!("[info] listen loop");

            stream.pause().expect("failed to pause the input stream");

            loop {
                let ctrl = receiver_ctrl
                    .recv()
                    .expect("failed to receive from control channel");

                match ctrl {
                    Control::Play => stream.play().expect("failed to play the input stream"),
                    Control::Pause => stream.pause().expect("failed to pause the input stream"),
                };
            }
        });

        Ok(IsRecordingCtrl { tx: send_ctrl })
    }

    fn err_fn(err: cpal::StreamError) {
        eprintln!("[error] an error occurred on stream: {}", err);
    }
}
