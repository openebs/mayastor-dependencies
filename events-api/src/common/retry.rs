use std::{cmp::min, time::Duration};

/// Backoff delay options for the retries to the NATS jetstream message bus.
/// Max number of retries until it gives up.
#[derive(Clone, Debug)]
pub struct BackoffOptions {
    /// Initial delay.
    pub init_delay: Duration,
    /// The number of attempts with initial delay.
    pub cutoff: u32,
    /// Increase in delay with each retry after cutoff is reached.
    pub step: Duration,
    /// Maximum delay.
    pub max_delay: Duration,
    /// Maximum retries.
    pub max_retries: u32,
}

impl Default for BackoffOptions {
    fn default() -> Self {
        Self {
            init_delay: Duration::from_secs(5),
            cutoff: 4,
            step: Duration::from_secs(2),
            max_delay: Duration::from_secs(10),
            max_retries: 10,
        }
    }
}

impl BackoffOptions {
    /// New options with default values.
    #[must_use]
    pub fn new() -> Self {
        Default::default()
    }

    /// Initial delay before the first retry.
    pub fn with_init_delay(mut self, init_delay: Duration) -> Self {
        self.init_delay = init_delay;
        self
    }

    /// Delay multiplied at each iteration.
    pub fn with_delay_step(mut self, step: Duration) -> Self {
        self.step = step;
        self
    }

    /// Number of tries with the initial delay.
    pub fn with_cutoff(mut self, cutoff: u32) -> Self {
        self.cutoff = cutoff;
        self
    }

    /// Maximum delay.
    pub fn with_max_delay(mut self, max_delay: Duration) -> Self {
        self.max_delay = max_delay;
        self
    }

    pub fn publish_backoff_options() -> Self {
        Self {
            init_delay: Duration::from_secs(5),
            cutoff: 4,
            step: Duration::from_secs(2),
            max_delay: Duration::from_secs(10),
            max_retries: 10,
        }
    }
}

/// Simple backoff delay which get gradually larger up to a 'max' duration.
pub async fn backoff_with_options(tries: &mut u32, options: &BackoffOptions) {
    *tries += 1;
    let backoff = if *tries <= options.cutoff {
        options.init_delay
    } else {
        min(
            options.init_delay + (*tries - options.cutoff - 1) * options.step,
            options.max_delay,
        )
    };
    tokio::time::sleep(backoff).await;
}
