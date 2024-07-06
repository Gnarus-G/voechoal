use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use crate::background::procedure::BackgroundProcedure;

pub enum StreamControlCommand {
    /// play audio item by id
    Play(String),
    Pause(Option<String>),
}

pub struct AudioCtrls {
    pub player: BackgroundProcedure<Option<String>, StreamControlCommand>,
    pub ecouter: BackgroundProcedure<Vec<f32>, StreamControlCommand>,
    pub sttlistener: BackgroundProcedure<(), StreamControlCommand>,
    pub db: Arc<Mutex<database::FSDatabase>>,
}

pub fn setup() -> anyhow::Result<AudioCtrls> {
    let db = Arc::new(Mutex::new(database::FSDatabase::new()));
    let host = cpal::default_host();
    let sttlistener = stt::listener::setup(&host, db.clone());
    let ectrl = ecouter::setup(&host, db.clone())?;
    let pctrl = player::setup(&host, db.clone())?;

    return Ok(AudioCtrls {
        player: pctrl,
        ecouter: ectrl,
        sttlistener,
        db,
    });
}

pub mod player {
    use std::{
        fs,
        io::BufReader,
        sync::{mpsc::channel, Arc, Mutex},
    };

    use anyhow::{anyhow, Context};
    use cpal::traits::{DeviceTrait, HostTrait};

    use crate::{audio::database::UpdateParams, background::procedure::BackgroundProcedure};

    use super::{app_dir, database::FSDatabase, StreamControlCommand};

    pub fn setup(
        host: &cpal::Host,
        db: Arc<Mutex<FSDatabase>>,
    ) -> anyhow::Result<BackgroundProcedure<Option<String>, StreamControlCommand>> {
        let speakers = host
            .default_output_device()
            .expect("no input device available");

        let supported_config = speakers
            .default_output_config()
            .expect("no supported ouput config?!");

        let config: cpal::StreamConfig = supported_config.into();
        eprintln!("[debug] output config: {:?}", config);

        // Thread that handles play/pause commands
        let job_handle =
            BackgroundProcedure::<Option<String>, StreamControlCommand>::setup(None, move |arg| {
                let current_audio_item_id = arg.state;
                let current_audio_item_id_clone = Arc::clone(&current_audio_item_id);

                let (_stream, handle) = rodio::OutputStream::try_default().unwrap();
                let sink = Arc::new(rodio::Sink::try_new(&handle).unwrap());
                let sink_ = Arc::clone(&sink);

                let (tx_id, rx_id) = channel::<String>();

                let db_for_closure = Arc::clone(&db);
                let _ = std::thread::spawn(move || loop {
                    let id = rx_id.recv().unwrap();
                    let wavfilepath = app_dir().join(&id).with_extension("wav");

                    let file = fs::File::open(&wavfilepath)
                        .context(anyhow!("failed to open file: {:?}", wavfilepath))
                        .expect("failed to open wav file for reading");

                    eprintln!("[info] audio item {} is playing", id);

                    let dec = rodio::Decoder::new(BufReader::new(file)).unwrap();
                    sink_.append(dec);

                    sink_.play();
                    sink_.sleep_until_end();

                    eprintln!("[info] audio item {} is done playing", id);

                    db_for_closure
                        .lock()
                        .unwrap()
                        .update_audio_items(UpdateParams {
                            id: &id,
                            is_playing: Some(false),
                            filepath: None,
                            label: None,
                        })
                        .expect("failed to mark audio item as paused");

                    *current_audio_item_id_clone.lock().unwrap() = None;
                });

                eprintln!("[info] player is ready");
                loop {
                    let ctrl = arg.rx.recv();

                    match ctrl {
                        Ok(StreamControlCommand::Play(id)) => {
                            eprintln!("[info] requested to play item: {}", id);

                            db.lock()
                                .unwrap()
                                .update_audio_items(UpdateParams {
                                    id: &id,
                                    is_playing: Some(true),
                                    filepath: None,
                                    label: None,
                                })
                                .expect("failed to mark audio item as playing");

                            if current_audio_item_id.lock().unwrap().as_ref() != Some(&id) {
                                *current_audio_item_id.lock().unwrap() = Some(id.clone());
                                sink.stop();

                                tx_id.send(id).expect("failed to send audio item id");
                            }

                            sink.play();
                        }
                        Ok(StreamControlCommand::Pause(id)) => {
                            eprintln!("[info] requested to pause item: {:?}", id);

                            sink.pause();

                            if let Some(id) = id {
                                db.lock()
                                    .unwrap()
                                    .update_audio_items(UpdateParams {
                                        id: &id,
                                        is_playing: Some(false),
                                        filepath: None,
                                        label: None,
                                    })
                                    .expect("failed to mark audio item as paused");
                            }
                        }
                        Err(err) => {
                            eprintln!("[error] recieve err on channel: {}", err);
                            return;
                        }
                    };
                }
            });

        Ok(job_handle)
    }
}

pub mod ecouter {
    use std::sync::{Arc, Mutex};

    use cpal::traits::StreamTrait;

    use crate::{
        audio::{self, audio_stream_err_fn, database::wav_spec_from},
        background::procedure::BackgroundProcedure,
    };

    use super::{database::FSDatabase, StreamControlCommand};

    pub fn setup(
        host: &cpal::Host,
        db: Arc<Mutex<FSDatabase>>,
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
                            // eprintln!("[info]: data len {:?}", &data.len());
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

                let pause = |stream: &cpal::Stream,
                             config: &cpal::StreamConfig,
                             new_audio_item_id: String| {
                    stream.pause().expect("failed to pause the input stream");
                    eprintln!("[info] done listening");

                    let audio_item = audio::AudioItem::new(new_audio_item_id);

                    eprintln!("[info] write wav file for new audio item");
                    {
                        db.lock().unwrap().write_to_wav(
                            &audio_item,
                            &audio_buffer.lock().expect("failed to lock on audio_buffer"),
                            wav_spec_from(config),
                        );
                    }

                    eprintln!("[info] saving audio item");
                    db.lock()
                        .unwrap()
                        .save_audio_item(audio_item)
                        .expect("failed to to save new audio item");

                    audio_buffer
                        .lock()
                        .expect("failed to lock on audio_buffer")
                        .clear();
                    eprintln!("[trace] cleared audio_buffer");
                };

                let mut current_new_audio_item_id = None;
                loop {
                    let ctrl = arg
                        .rx
                        .recv()
                        .expect("failed to receive from control channel");

                    match ctrl {
                        StreamControlCommand::Play(id) => {
                            eprintln!("[info] listening...");
                            current_new_audio_item_id = Some(id);
                            stream.play().expect("failed to play the input stream");
                        }
                        StreamControlCommand::Pause(_) => {
                            if let Some(id) = current_new_audio_item_id.clone() {
                                pause(&stream, &config, id);
                            }
                        }
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
        items: Vec<AudioItem>,
    }

    pub struct UpdateParams<'i> {
        pub id: &'i str,
        pub filepath: Option<&'i Path>,
        pub is_playing: Option<bool>,
        pub label: Option<String>,
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
                items: Self::load_all().unwrap(),
            }
        }

        pub fn items(&self) -> Vec<AudioItem> {
            self.items.clone()
        }

        pub fn remove_item(&mut self, id: String) {
            self.items
                .iter()
                .position(|i| i.id == id)
                .map(|i| self.items.remove(i));
        }

        pub fn write_to_wav(&mut self, item: &AudioItem, buffer: &[f32], spec: hound::WavSpec) {
            eprintln!("[info] writing wav with specs: {:?}", spec);
            let wav_filepath = self.wav_dir.join(&item.id).with_extension("wav");
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
            self.items.push(item);

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
            for item in self.items.iter_mut() {
                if item.id == params.id {
                    item.is_playing = params.is_playing.unwrap_or(item.is_playing);
                    item.filepath = params
                        .filepath
                        .map(|p| p.to_path_buf())
                        .unwrap_or(item.filepath.clone());
                    if let Some(e) = params.label {
                        item.excerpt = Some(e);
                    }

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

    pub mod listener {
        use core::f32;
        use std::{
            ops::Deref,
            sync::{Arc, Mutex},
        };

        use cpal::traits::{HostTrait, StreamTrait};
        use rodio::DeviceTrait;

        use crate::{
            audio::{audio_stream_err_fn, database::FSDatabase, StreamControlCommand},
            background::procedure::BackgroundProcedure,
            sharedref::SharedMutRef,
        };

        const WHISPER_SAMPLE_RATE: u32 = 16000;
        const MAX_AUDIO_LEN_SECONDS: u32 = 5;
        const WHISPER_CHANNEL_COUNT: u16 = 1; // mono because whisper wants it

        pub fn setup(
            host: &cpal::Host,
            db: Arc<Mutex<FSDatabase>>,
        ) -> BackgroundProcedure<(), StreamControlCommand> {
            let mic = host
                .default_input_device()
                .expect("failed to get default input device");

            let job = BackgroundProcedure::<_, StreamControlCommand>::setup((), move |arg| {
                let buffer_size = WHISPER_SAMPLE_RATE * MAX_AUDIO_LEN_SECONDS;
                let config: cpal::StreamConfig = cpal::StreamConfig {
                    channels: WHISPER_CHANNEL_COUNT,
                    sample_rate: cpal::SampleRate(WHISPER_SAMPLE_RATE),
                    buffer_size: cpal::BufferSize::Fixed(buffer_size),
                };

                struct Buffer {
                    cap: u32,
                    inner: Vec<f32>,
                }

                impl Buffer {
                    fn new(cap: u32) -> Self {
                        Self {
                            inner: Vec::with_capacity(cap as usize),
                            cap,
                        }
                    }

                    fn is_full(&self) -> bool {
                        self.inner.len() >= self.cap as usize
                    }

                    fn extends(&mut self, data: &[f32]) {
                        self.inner.extend(data);
                    }
                }

                let db_ = Arc::clone(&db);

                let buffer = Arc::new(Mutex::new(Buffer::new(buffer_size)));
                let buffer_clone1 = Arc::clone(&buffer);
                let audio_item_id = SharedMutRef::<Option<String>>::new(None);
                let audio_item_id_ref = audio_item_id.new_ref();
                let stream = mic
                    .build_input_stream(
                        &config,
                        move |data: &[f32], _| {
                            eprintln!("[info] data len: {}", data.len());
                            if !buffer_clone1.lock().unwrap().is_full() {
                                buffer_clone1.lock().unwrap().extends(data)
                            }
                        },
                        audio_stream_err_fn,
                        None,
                    )
                    .expect("failed to build input stream to listen for stt");

                stream.pause().expect("failed to pause stream");

                let job = BackgroundProcedure::<_, Vec<f32>>::setup((), move |arg| {
                    let tt = super::Transcribe::new("/home/gnarus/d/caldi/models/ggml-base.en.bin");
                    let prompt = r#"[system]
                                    Transcribe the first 24 words in the song that the user is singing.
                                    [user]"#;

                    loop {
                        let buffer = arg.rx.recv().expect("failed to recieve from channel");
                        if buffer.is_empty() {
                            continue;
                        }
                        let transcript = tt.transcribe(&buffer, prompt);

                        db_.lock()
                            .unwrap()
                            .update_audio_items(crate::audio::database::UpdateParams {
                                id: audio_item_id_ref.deref().lock().unwrap().as_ref().unwrap(),
                                filepath: None,
                                is_playing: None,
                                label: Some(transcript),
                            })
                            .expect("failed to update audio items");
                    }
                });

                let mut is_done_transcribing = false;
                loop {
                    let command = arg.rx.try_recv();

                    match command {
                        Ok(StreamControlCommand::Play(id)) => {
                            eprintln!("[info] stt is listening...");
                            *audio_item_id.deref().lock().unwrap() = Some(id);
                            stream.play().expect("failed to play stream");
                            is_done_transcribing = false;
                        }
                        Ok(StreamControlCommand::Pause(_)) => {
                            eprintln!("[info] stt is done listening");
                            stream.pause().expect("failed to pause stream");

                            job.trigger(buffer.lock().unwrap().inner.clone());
                            buffer.lock().unwrap().inner.clear();

                            is_done_transcribing = true;
                        }
                        Err(std::sync::mpsc::TryRecvError::Empty) => {
                            if buffer.lock().unwrap().is_full() && !is_done_transcribing {
                                eprintln!("[info] stt is done listening");
                                stream.pause().expect("failed to pause stream");

                                job.trigger(buffer.lock().unwrap().inner.clone());
                                buffer.lock().unwrap().inner.clear();

                                is_done_transcribing = true;
                            }
                        }
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => return,
                    }
                }
            });

            return job;
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AudioItem {
    pub id: String,
    pub excerpt: Option<String>,
    pub filepath: PathBuf,
    #[serde(default)]
    pub is_playing: bool,
}

impl AudioItem {
    pub fn new(id: String) -> Self {
        Self {
            filepath: app_dir().join(&id).with_extension("wav"),
            id,
            excerpt: None,
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
