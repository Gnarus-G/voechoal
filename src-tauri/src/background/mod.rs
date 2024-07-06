pub mod procedure {
    use std::{
        sync::{
            mpsc::{channel, Receiver, Sender},
            Arc, Mutex,
        },
        thread,
    };

    pub struct BackgroundProcedure<S: Sync + Send, C: Send> {
        pub state: Arc<Mutex<S>>,
        pub tx: Sender<C>,
    }

    pub struct WorkArgs<S, C: Send> {
        pub state: Arc<Mutex<S>>,
        pub rx: Receiver<C>,
    }

    #[derive(Debug, thiserror::Error)]
    pub enum TryResponseError {
        #[error("no response yet")]
        Nothing,
        #[error("channel is disconnected")]
        Disconnected,
    }

    impl<S: Send + Sync + 'static, C: Send + 'static> BackgroundProcedure<S, C> {
        pub fn setup<F: Fn(WorkArgs<S, C>) + std::marker::Send + 'static>(
            state: S,
            work: F,
        ) -> Self {
            let (tx, rx) = channel::<C>();
            let job = Self {
                state: Arc::new(Mutex::new(state)),
                tx,
            };

            let state_ref = Arc::clone(&job.state);
            let _joinhandle = thread::spawn(move || {
                work(WorkArgs {
                    state: state_ref,
                    rx,
                });
            });

            job
        }

        pub fn trigger(&self, command: C) {
            self.tx
                .send(command)
                .expect("failed to send through channel");
        }
    }
}

pub mod job {

    use std::{
        sync::{
            mpsc::{channel, Receiver, RecvError, Sender},
            Arc, Mutex,
        },
        thread,
    };

    pub struct BackgroundJob<S: Sync + Send, C: Send, R: Send> {
        pub state: Arc<Mutex<S>>,
        tx: Sender<C>,
        rx: Receiver<R>,
        // _joinhandle: JoinHandle<()>,
    }

    pub struct WorkArgs<S, C: Send, R: Send> {
        pub state: Arc<Mutex<S>>,
        pub rx: Receiver<C>,
        pub tx: Sender<R>,
    }

    #[derive(Debug, thiserror::Error)]
    pub enum TryResponseError {
        #[error("no response yet")]
        Nothing,
        #[error("channel is disconnected")]
        Disconnected,
    }

    impl<S: Send + Sync + 'static, C: Send + 'static, R: Send + 'static> BackgroundJob<S, C, R> {
        pub fn setup<F: Fn(WorkArgs<S, C, R>) + std::marker::Send + 'static>(
            state: S,
            work: F,
        ) -> Self {
            let (tx, rx) = channel::<C>();
            let (tx_prime, rx_prime) = channel::<R>();
            let job = Self {
                state: Arc::new(Mutex::new(state)),
                tx,
                rx: rx_prime,
            };

            let state_ref = Arc::clone(&job.state);
            let _joinhandle = thread::spawn(move || {
                work(WorkArgs {
                    state: state_ref,
                    rx,
                    tx: tx_prime,
                });
            });

            job
        }

        pub fn trigger(&self, command: C) {
            self.tx
                .send(command)
                .expect("failed to send through channel");
        }

        pub fn wait_for_response(&self) -> Result<R, RecvError> {
            self.rx.recv()
        }

        pub fn try_response(&self) -> Result<R, TryResponseError> {
            match self.rx.try_recv() {
                Ok(r) => Ok(r),
                Err(err) => match err {
                    std::sync::mpsc::TryRecvError::Empty => Err(TryResponseError::Nothing),
                    std::sync::mpsc::TryRecvError::Disconnected => {
                        Err(TryResponseError::Disconnected)
                    }
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::job::BackgroundJob;
    use super::procedure::BackgroundProcedure;

    #[test]
    fn it_works_procedure() {
        let job = BackgroundProcedure::<_, i8>::setup(1, |arg| {
            let command = arg.rx.recv().unwrap();
            assert_eq!(command, 0);

            let command = arg.rx.recv().unwrap();
            assert_eq!(command, 1);

            let command = arg.rx.recv().unwrap();
            assert_eq!(command, 2);

            *arg.state.lock().unwrap() = 420;
        });

        job.trigger(0);
        job.trigger(1);
        job.trigger(2);

        let s = *job.state.lock().unwrap();
        assert_eq!(s, 420);
    }

    #[test]
    fn it_works_job() {
        let job = BackgroundJob::<_, i8, char>::setup(1, |arg| {
            let command = arg.rx.recv().unwrap();
            assert_eq!(command, 0);

            let command = arg.rx.recv().unwrap();
            assert_eq!(command, 1);

            arg.tx.send('a').unwrap();

            let command = arg.rx.recv().unwrap();
            assert_eq!(command, 2);

            arg.tx.send('b').unwrap();

            *arg.state.lock().unwrap() = 420;
        });

        job.trigger(0);
        job.trigger(1);
        job.trigger(2);

        let r = job.wait_for_response().unwrap();
        assert_eq!(r, 'a');

        while let Ok(r) = job.try_response() {
            assert_eq!(r, 'b');
        }

        job.try_response().unwrap_err();

        let s = *job.state.lock().unwrap();
        assert_eq!(s, 420);
    }
}
