use oneshim_api_contracts::idle::IdlePeriodResponse;

use crate::error::ApiError;
use crate::services::idle_assembler::assemble_idle_period_response;
use crate::services::web_contexts::StorageWebContext;
use oneshim_api_contracts::common::TimeRangeQuery;

#[derive(Clone)]
pub struct IdleQueryService {
    ctx: StorageWebContext,
}

impl IdleQueryService {
    pub fn new(ctx: StorageWebContext) -> Self {
        Self { ctx }
    }

    pub async fn get_idle_periods(
        &self,
        params: &TimeRangeQuery,
    ) -> Result<Vec<IdlePeriodResponse>, ApiError> {
        let from = params.from_datetime();
        let to = params.to_datetime();

        self.ctx
            .storage
            .get_idle_periods(from, to)
            .await
            .map_err(ApiError::from)
            .map(|periods| {
                periods
                    .into_iter()
                    .map(assemble_idle_period_response)
                    .collect()
            })
    }
}
