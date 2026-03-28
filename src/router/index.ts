import { createRouter, createWebHashHistory } from 'vue-router'
import MainWindow from '@/views/MainWindow.vue'
import TabWindow from '@/views/TabWindow.vue'
import SettingsWindow from '@/views/SettingsWindow.vue'

const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    {
      path: '/',
      name: 'main',
      component: MainWindow
    },
    {
      path: '/tab/:type',
      name: 'tab',
      component: TabWindow,
      props: true
    },
    {
      path: '/settings',
      name: 'settings',
      component: SettingsWindow
    },
    {
      path: '/archive',
      name: 'archive',
      component: () => import('@/views/ArchiveWindow.vue')
    }
  ]
})

export default router
