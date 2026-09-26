/** 投票状态直接订阅扩展宿主。 */
import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { createExtensionClient } from '@/services/extensions'
import type { Poll, Voter, VoteKeyType } from '@/types/extensions'

export const useVotingStore = defineStore('voting', () => {
  const polls = ref<Poll[]>([])
  const error = ref('')
  const client = createExtensionClient('voting', state => { polls.value = state }, message => { error.value = message })
  const activePolls = computed(() => polls.value.filter(p => p.status === 'active'))
  const endedPolls = computed(() => polls.value.filter(p => p.status === 'ended'))
  const hasActivePolls = computed(() => activePolls.value.length > 0)
  const createPoll = (title: string, options: [string, string][], keyType: VoteKeyType, durationSecs?: number) =>
    client.request<Poll>({ type: 'create', title, options, key_type: keyType, duration_secs: durationSecs ?? null })
  const endPoll = (pollId: string) => client.request({ type: 'end', poll_id: pollId })
  const deletePoll = (pollId: string) => client.request({ type: 'delete', poll_id: pollId })
  const getVoters = (pollId: string, optionKey: string) => client.query<Voter[]>({ type: 'voters', poll_id: pollId, option_key: optionKey })
  return { polls, error, activePolls, endedPolls, hasActivePolls,
    connect: client.connect, disconnect: client.disconnect, createPoll, endPoll, deletePoll, getVoters }
})
