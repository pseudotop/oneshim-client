/**
 * 공통 포맷터 유틸리티
 *
 * 전체 프론트엔드에서 사용되는 포맷 함수 모음
 */

/** 초 → 사람 읽기 형식 (예: "2h 30m", "2h 30m 15s") */
export function formatDuration(secs: number, short = false): string {
  const hours = Math.floor(secs / 3600)
  const minutes = Math.floor((secs % 3600) / 60)
  const seconds = secs % 60

  if (short) {
    if (hours > 0) return `${hours}h ${minutes}m`
    return `${minutes}m`
  }

  if (hours > 0) return `${hours}h ${minutes}m ${seconds}s`
  if (minutes > 0) return `${minutes}m ${seconds}s`
  return `${seconds}s`
}

/** 바이트 → 사람 읽기 형식 (예: "1.5MB", "2.30GB") */
export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes}B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)}KB`
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)}MB`
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)}GB`
}

/** 타임스탬프 → 시간만 표시 (예: "14:30:05") */
export function formatTime(timestamp: string): string {
  const date = new Date(timestamp)
  return date.toLocaleTimeString('ko-KR', {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hour12: false,
  })
}

/** 타임스탬프 → 날짜+시간 표시 (예: "1월 5일 14:30") */
export function formatDateTime(timestamp: string): string {
  const date = new Date(timestamp)
  return date.toLocaleString('ko-KR', {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  })
}

/** 타임스탬프 → 날짜만 표시 (예: "1월 5일") */
export function formatDate(timestamp: string | undefined): string {
  if (!timestamp) return new Date().toISOString().split('T')[0]
  const date = new Date(timestamp)
  return date.toLocaleDateString('ko-KR', { month: 'short', day: 'numeric' })
}

/** 숫자 → 로케일 형식 (예: "1,234,567") */
export function formatNumber(num: number): string {
  return num.toLocaleString('ko-KR')
}

/** 시간 문자열 → 표시 형식 (예: "14시") */
export function formatHour(hourStr: string): string {
  const hour = parseInt(hourStr, 10)
  if (isNaN(hour)) return hourStr
  return `${hour}시`
}

/** 비율 → 퍼센트 표시 (예: "85.5%") */
export function formatPercent(value: number, decimals = 1): string {
  return `${value.toFixed(decimals)}%`
}

/** 정규식 특수문자 이스케이프 */
export function escapeRegex(str: string): string {
  return str.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
}
