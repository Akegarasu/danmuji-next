<script setup lang="ts">
import { computed, onMounted, onUnmounted } from 'vue'
import { getCurrentWindow } from '@tauri-apps/api/window'
import TitleBar from '@/components/common/TitleBar.vue'
import { getExtensionSettings } from '@/components/extension/settings-registry'
import { initWindowManager, cleanupWindowManager, closeWindow } from '@/services/window-manager'

const props = defineProps<{ extensionId: string }>()
const definition = computed(() => getExtensionSettings(props.extensionId))
const label = getCurrentWindow().label
onMounted(() => { void initWindowManager(label) })
onUnmounted(() => { void cleanupWindowManager(label) })
</script>

<template>
  <div class="extension-settings-window">
    <TitleBar :title="definition?.title ?? '扩展设置'" is-sub-window :window-label="label" />
    <component :is="definition.component" v-if="definition" @close="closeWindow(label)" />
    <p v-else class="unavailable" role="alert">此扩展尚未提供设置页面。</p>
  </div>
</template>

<style scoped>
.extension-settings-window { display: flex; flex-direction: column; height: 100%; background: var(--bg-primary); border: var(--window-border); border-radius: var(--border-radius); overflow: hidden; }
.unavailable { padding: 24px; color: var(--text-secondary); }
</style>
