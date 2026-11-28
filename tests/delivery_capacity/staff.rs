use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Router;
use sqlx::SqlitePool;
use tokio::sync::mpsc;

use crate::fixture::Tenant;
use crate::http::complete;
use crate::report;

const WINDOW_SECONDS: u64 = 60;

pub struct StaffOrder {
    pub tenant_index: usize,
    pub public_id: String,
    pub submitted_at: Instant,
}

pub struct StaffSample {
    handoff: Vec<Duration>,
    service: Vec<Duration>,
    completion_windows: Vec<usize>,
}

impl StaffSample {
    pub fn completed(&self) -> usize {
        self.service.len()
    }

    pub fn report(mut self, elapsed: Duration) {
        report(&mut self.handoff, "staff queue wait", elapsed);
        report(&mut self.service, "staff processing per order", elapsed);
        for (window, orders) in self.completion_windows.into_iter().enumerate() {
            let start = window as u64 * WINDOW_SECONDS;
            let end = elapsed.as_secs_f64().min((start + WINDOW_SECONDS) as f64);
            println!("staff completions at {start}–{end:.1}s: {orders}");
        }
    }
}

pub async fn process(
    app: Router,
    pool: SqlitePool,
    tenants: Arc<Vec<Tenant>>,
    mut queue: mpsc::Receiver<StaffOrder>,
    started: Instant,
) -> StaffSample {
    let mut sample = StaffSample {
        handoff: Vec::new(),
        service: Vec::new(),
        completion_windows: Vec::new(),
    };
    while let Some(order) = queue.recv().await {
        let received = Instant::now();
        sample
            .handoff
            .push(received.duration_since(order.submitted_at));
        complete(&app, &pool, &tenants[order.tenant_index], &order.public_id).await;
        let finished = Instant::now();
        sample.service.push(finished.duration_since(received));
        let window = (finished.duration_since(started).as_secs() / WINDOW_SECONDS) as usize;
        if window >= sample.completion_windows.len() {
            sample.completion_windows.resize(window + 1, 0);
        }
        sample.completion_windows[window] += 1;
    }
    sample
}
