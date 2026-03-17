use oneshim_api_contracts::export::{EventExportRecord, FrameExportRecord, MetricExportRecord};
use oneshim_core::models::storage_records::{
    EventExportRecord as EventExportRow, FrameExportRecord as FrameExportRow,
    MetricExportRecord as MetricExportRow,
};

pub(crate) fn assemble_metric_export_record(row: MetricExportRow) -> MetricExportRecord {
    let memory_percent = if row.memory_total > 0 {
        (row.memory_used as f32 / row.memory_total as f32) * 100.0
    } else {
        0.0
    };

    MetricExportRecord {
        timestamp: row.timestamp,
        cpu_usage: row.cpu_usage,
        memory_used: row.memory_used,
        memory_total: row.memory_total,
        memory_percent,
        disk_used: row.disk_used,
        disk_total: row.disk_total,
        network_upload: row.network_upload,
        network_download: row.network_download,
    }
}

pub(crate) fn assemble_event_export_record(row: EventExportRow) -> EventExportRecord {
    EventExportRecord {
        event_id: row.event_id,
        event_type: row.event_type,
        timestamp: row.timestamp,
        app_name: row.app_name,
        window_title: row.window_title,
    }
}

pub(crate) fn assemble_frame_export_record(row: FrameExportRow) -> FrameExportRecord {
    FrameExportRecord {
        id: row.id,
        timestamp: row.timestamp,
        trigger_type: row.trigger_type,
        app_name: row.app_name,
        window_title: row.window_title,
        importance: row.importance,
        resolution: format!("{}x{}", row.resolution_w, row.resolution_h),
        ocr_text: row.ocr_text,
    }
}
