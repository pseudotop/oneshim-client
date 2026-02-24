//!

use std::collections::VecDeque;

pub const METRICS_HISTORY_SIZE: usize = 60;

#[derive(Debug, Clone)]
pub struct MainWindowState {
    pub is_visible: bool,
    pub connection_status: String,
    pub active_app: Option<String>,

    pub cpu_usage: f32,
    pub memory_usage_mb: f64,
    pub cpu_history: VecDeque<f32>,
    pub memory_history: VecDeque<f64>,

    pub system_cpu_usage: f32,
    pub system_memory_used_mb: f64,
    pub system_memory_total_mb: f64,
    pub system_cpu_history: VecDeque<f32>,
    pub system_memory_history: VecDeque<f64>,

    pub recent_suggestion_count: usize,
}

impl MainWindowState {
    pub fn new() -> Self {
        Self {
            is_visible: false,
            connection_status: "connection 안됨".to_string(),
            active_app: None,

            cpu_usage: 0.0,
            memory_usage_mb: 0.0,
            cpu_history: VecDeque::with_capacity(METRICS_HISTORY_SIZE),
            memory_history: VecDeque::with_capacity(METRICS_HISTORY_SIZE),

            system_cpu_usage: 0.0,
            system_memory_used_mb: 0.0,
            system_memory_total_mb: 0.0,
            system_cpu_history: VecDeque::with_capacity(METRICS_HISTORY_SIZE),
            system_memory_history: VecDeque::with_capacity(METRICS_HISTORY_SIZE),

            recent_suggestion_count: 0,
        }
    }

    pub fn update_connection(&mut self, status: &str) {
        self.connection_status = status.to_string();
    }

    pub fn update_metrics(
        &mut self,
        agent_cpu: f32,
        agent_memory_mb: f64,
        system_cpu: f32,
        system_memory_used_mb: f64,
        system_memory_total_mb: f64,
    ) {
        self.cpu_usage = agent_cpu;
        self.memory_usage_mb = agent_memory_mb;

        if self.cpu_history.len() >= METRICS_HISTORY_SIZE {
            self.cpu_history.pop_front();
        }
        self.cpu_history.push_back(agent_cpu);

        if self.memory_history.len() >= METRICS_HISTORY_SIZE {
            self.memory_history.pop_front();
        }
        self.memory_history.push_back(agent_memory_mb);

        self.system_cpu_usage = system_cpu;
        self.system_memory_used_mb = system_memory_used_mb;
        self.system_memory_total_mb = system_memory_total_mb;

        if self.system_cpu_history.len() >= METRICS_HISTORY_SIZE {
            self.system_cpu_history.pop_front();
        }
        self.system_cpu_history.push_back(system_cpu);

        if self.system_memory_history.len() >= METRICS_HISTORY_SIZE {
            self.system_memory_history.pop_front();
        }
        self.system_memory_history.push_back(system_memory_used_mb);
    }

    pub fn cpu_history_slice(&self) -> Vec<f32> {
        self.cpu_history.iter().copied().collect()
    }

    pub fn memory_history_slice(&self) -> Vec<f64> {
        self.memory_history.iter().copied().collect()
    }

    pub fn system_cpu_history_slice(&self) -> Vec<f32> {
        self.system_cpu_history.iter().copied().collect()
    }

    pub fn system_memory_history_slice(&self) -> Vec<f64> {
        self.system_memory_history.iter().copied().collect()
    }
}

impl Default for MainWindowState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_state() {
        let state = MainWindowState::new();
        assert!(!state.is_visible);
        assert_eq!(state.connection_status, "connection 안됨");
    }

    #[test]
    fn update_metrics() {
        let mut state = MainWindowState::new();
        // agent_cpu, agent_mem, sys_cpu, sys_mem_used, sys_mem_total
        state.update_metrics(5.0, 128.5, 45.0, 8192.0, 16384.0);
        assert_eq!(state.cpu_usage, 5.0);
        assert_eq!(state.memory_usage_mb, 128.5);
        assert_eq!(state.system_cpu_usage, 45.0);
        assert_eq!(state.system_memory_used_mb, 8192.0);
        assert_eq!(state.system_memory_total_mb, 16384.0);
    }

    #[test]
    fn metrics_history() {
        let mut state = MainWindowState::new();
        for i in 0..5 {
            state.update_metrics(
                i as f32,
                (i * 10) as f64,
                (i * 2) as f32,
                (i * 100) as f64,
                16384.0,
            );
        }
        assert_eq!(state.cpu_history.len(), 5);
        assert_eq!(state.memory_history.len(), 5);
        assert_eq!(state.cpu_history_slice(), vec![0.0, 1.0, 2.0, 3.0, 4.0]);

        assert_eq!(state.system_cpu_history.len(), 5);
        assert_eq!(state.system_memory_history.len(), 5);
        assert_eq!(
            state.system_cpu_history_slice(),
            vec![0.0, 2.0, 4.0, 6.0, 8.0]
        );
    }
}
