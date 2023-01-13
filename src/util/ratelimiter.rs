use tokio::{
    sync::Mutex,
    time::{sleep, Duration, Instant},
};

pub struct Ratelimiter {
    pub duration: Duration,
    pub count: usize,

    value: Mutex<(Instant, usize)>,
}

impl Ratelimiter {
    pub fn new(duration: Duration, count: usize) -> Ratelimiter {
        Ratelimiter {
            duration,
            count,
            value: Mutex::new((Instant::now(), 0)),
        }
    }
    pub async fn ask(&self) {
        loop {
            let now = Instant::now();

            let left = {
                let mut v = self.value.lock().await;

                if now - v.0 > self.duration {
                    *v = (now, 0);
                    return;
                } else if v.1 < self.count - 1 {
                    (*v).1 += 1;
                    return;
                }

                self.duration - (now - v.0)
            };

            sleep(left).await;
        }
    }
}
