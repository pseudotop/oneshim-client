import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type { TimelineItem } from '../../../api/client'
import type { FrameItem, PlaybackState } from '../types'

interface TimelineData {
  session: { start: string; end: string }
  items: TimelineItem[]
}

/**
 * Encapsulates all playback-specific state: play/pause, speed, currentTime,
 * frame lookup, and keyboard-style navigation (skip to start/end).
 */
export function usePlaybackState(timeline: TimelineData | undefined): PlaybackState {
  const [isPlaying, setIsPlaying] = useState(false)
  const [playbackSpeed, setPlaybackSpeed] = useState(1)
  const [currentTime, setCurrentTime] = useState<Date>(new Date())

  const playIntervalRef = useRef<number | null>(null)

  // Sync currentTime to session start when timeline loads
  useEffect(() => {
    if (timeline?.session) {
      setCurrentTime(new Date(timeline.session.start))
    }
  }, [timeline])

  const startTime = useMemo(() => {
    return timeline?.session ? new Date(timeline.session.start) : new Date()
  }, [timeline])

  const endTime = useMemo(() => {
    return timeline?.session ? new Date(timeline.session.end) : new Date()
  }, [timeline])

  // Find the closest frame at or before currentTime
  const currentFrame = useMemo((): FrameItem | null => {
    if (!timeline?.items) return null

    const frames = timeline.items.filter((item): item is FrameItem => item.type === 'Frame')

    if (frames.length === 0) return null

    const currentMs = currentTime.getTime()

    let closest: FrameItem | null = null
    let closestDiff = Infinity

    for (const frame of frames) {
      const frameMs = new Date(frame.timestamp).getTime()
      if (frameMs <= currentMs) {
        const diff = currentMs - frameMs
        if (diff < closestDiff) {
          closestDiff = diff
          closest = frame
        }
      }
    }

    return closest
  }, [timeline, currentTime])

  // Playback interval timer
  useEffect(() => {
    if (isPlaying && timeline) {
      playIntervalRef.current = window.setInterval(() => {
        setCurrentTime((prev) => {
          const newTime = new Date(prev.getTime() + playbackSpeed * 1000)
          if (newTime >= endTime) {
            setIsPlaying(false)
            return endTime
          }
          return newTime
        })
      }, 1000)
    } else {
      if (playIntervalRef.current) {
        clearInterval(playIntervalRef.current)
        playIntervalRef.current = null
      }
    }

    return () => {
      if (playIntervalRef.current) {
        clearInterval(playIntervalRef.current)
      }
    }
  }, [isPlaying, playbackSpeed, endTime, timeline])

  const handlePlayPause = useCallback(() => {
    setIsPlaying((prev) => !prev)
  }, [])

  const handleSpeedChange = useCallback((speed: number) => {
    setPlaybackSpeed(speed)
  }, [])

  const handleTimeChange = useCallback((time: Date) => {
    setCurrentTime(time)
  }, [])

  const handleSkipToStart = useCallback(() => {
    setCurrentTime(startTime)
    setIsPlaying(false)
  }, [startTime])

  const handleSkipToEnd = useCallback(() => {
    setCurrentTime(endTime)
    setIsPlaying(false)
  }, [endTime])

  return {
    isPlaying,
    playbackSpeed,
    currentTime,
    startTime,
    endTime,
    currentFrame,
    handlePlayPause,
    handleSpeedChange,
    handleTimeChange,
    handleSkipToStart,
    handleSkipToEnd,
  }
}
