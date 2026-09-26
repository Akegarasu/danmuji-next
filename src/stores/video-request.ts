/** 点播状态直接订阅扩展宿主。 */
import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { createExtensionClient } from '@/services/extensions'
import type { VideoRequestItem } from '@/types/extensions'

export const useVideoRequestStore = defineStore('videoRequest', () => {
  const requests = ref<VideoRequestItem[]>([])
  const error = ref('')
  const client = createExtensionClient('video-request', state => { requests.value = state }, message => { error.value = message })
  const unwatchedRequests = computed(() => requests.value.filter(r => !r.watched))
  const watchedRequests = computed(() => requests.value.filter(r => r.watched))
  const unwatchedCount = computed(() => unwatchedRequests.value.length)
  const markWatched = (id: string, watched = true) => client.request({ type: 'mark_watched', request_id: id, watched })
  const removeRequest = (id: string) => client.request({ type: 'remove', request_id: id })
  const clearWatched = () => client.request({ type: 'clear_watched' })
  const clearAll = () => client.request({ type: 'clear_all' })
  return { requests, error, unwatchedRequests, watchedRequests, unwatchedCount,
    connect: client.connect, disconnect: client.disconnect, markWatched, removeRequest, clearWatched, clearAll }
})
