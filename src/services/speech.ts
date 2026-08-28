import { invoke } from '@tauri-apps/api/core'
import type { SpeechStatus, SpeechVoice } from '@/types'

export const getSpeechVoices = (): Promise<SpeechVoice[]> =>
  invoke<SpeechVoice[]>('get_speech_voices')

export const previewSpeech = (voiceId: string | null, rate: number): Promise<void> =>
  invoke('preview_speech', { voiceId, rate })

export const getSpeechStatus = (): Promise<SpeechStatus> =>
  invoke<SpeechStatus>('get_speech_status')
