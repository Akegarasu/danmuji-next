import { invoke } from '@tauri-apps/api/core'

export interface RawDumpStatus {
  recording: boolean
  path: string | null
  event_count: number
  bytes_written: number
  dropped_events: number
  error: string | null
}

export const getRawDumpStatus = (): Promise<RawDumpStatus> => invoke('get_raw_dump_status')
export const startRawDump = (): Promise<RawDumpStatus> => invoke('start_raw_dump')
export const stopRawDump = (): Promise<RawDumpStatus> => invoke('stop_raw_dump')
export const openRawDumpDirectory = (): Promise<void> => invoke('open_raw_dump_directory')
