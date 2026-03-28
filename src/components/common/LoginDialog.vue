<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch } from 'vue'
import { 
  QRCodeLoginManager, 
  QRCodeStatusCode, 
  getUserInfo,
  type QRCodeData, 
  type QRCodeStatus, 
  type UserInfo 
} from '@/services/auth'

const props = defineProps<{
  visible: boolean
}>()

const emit = defineEmits<{
  (e: 'update:visible', value: boolean): void
  (e: 'login-success', cookie: string, userInfo: UserInfo): void
}>()

// ==================== 状态 ====================

type LoginState = 'loading' | 'waiting' | 'scanned' | 'success' | 'expired' | 'error'

const loginState = ref<LoginState>('loading')
const qrcodeUrl = ref('')
const statusMessage = ref('')
const errorMessage = ref('')
const userInfo = ref<UserInfo | null>(null)

// 登录管理器
const loginManager = new QRCodeLoginManager()
  .setQRCodeCallback((qrcode: QRCodeData) => {
    qrcodeUrl.value = qrcode.url
    loginState.value = 'waiting'
    statusMessage.value = '请使用哔哩哔哩 App 扫描二维码'
  })
  .setStatusCallback(async (status: QRCodeStatus) => {
    switch (status.code) {
      case QRCodeStatusCode.NOT_SCANNED:
        loginState.value = 'waiting'
        statusMessage.value = '请使用哔哩哔哩 App 扫描二维码'
        break
      case QRCodeStatusCode.SCANNED_NOT_CONFIRMED:
        loginState.value = 'scanned'
        statusMessage.value = '扫描成功，请在手机上确认登录'
        break
      case QRCodeStatusCode.EXPIRED:
        loginState.value = 'expired'
        statusMessage.value = '二维码已过期，请刷新'
        break
      case QRCodeStatusCode.SUCCESS:
        if (status.cookie) {
          loginState.value = 'success'
          statusMessage.value = '登录成功！'
          
          // 获取用户信息
          try {
            const info = await getUserInfo(status.cookie)
            userInfo.value = info
            
            // 延迟一下再关闭，让用户看到成功状态
            setTimeout(() => {
              emit('login-success', status.cookie!, info)
              close()
            }, 1000)
          } catch (e) {
            // 即使获取用户信息失败，Cookie 也是有效的
            setTimeout(() => {
              emit('login-success', status.cookie!, {
                uid: 0,
                uname: '未知用户',
                face: '',
                is_login: true
              })
              close()
            }, 1000)
          }
        }
        break
    }
  })
  .setErrorCallback((error: string) => {
    loginState.value = 'error'
    errorMessage.value = error
  })

// ==================== 计算属性 ====================

const stateIcon = computed(() => {
  switch (loginState.value) {
    case 'loading': return '⏳'
    case 'waiting': return '📱'
    case 'scanned': return '✅'
    case 'success': return '🎉'
    case 'expired': return '⏰'
    case 'error': return '❌'
  }
})

const showQRCode = computed(() => 
  loginState.value === 'waiting' || loginState.value === 'scanned'
)

const showRefreshButton = computed(() => 
  loginState.value === 'expired' || loginState.value === 'error'
)

// ==================== 方法 ====================

const startLogin = async () => {
  loginState.value = 'loading'
  statusMessage.value = '正在生成二维码...'
  errorMessage.value = ''
  await loginManager.start()
}

const refreshQRCode = async () => {
  await startLogin()
}

const close = () => {
  loginManager.stop()
  emit('update:visible', false)
}

// ==================== 生命周期 ====================

watch(() => props.visible, (visible) => {
  if (visible) {
    startLogin()
  } else {
    loginManager.stop()
  }
})

onMounted(() => {
  if (props.visible) {
    startLogin()
  }
})

onUnmounted(() => {
  loginManager.stop()
})
</script>

<template>
  <Teleport to="body">
    <Transition name="fade">
      <div v-if="visible" class="login-overlay" @click.self="close">
        <div class="login-dialog">
          <!-- 标题栏 -->
          <div class="dialog-header">
            <h3>扫码登录</h3>
            <button class="close-btn" @click="close">✕</button>
          </div>
          
          <!-- 内容区域 -->
          <div class="dialog-content">
            <!-- 二维码区域 -->
            <div class="qrcode-container" :class="{ dimmed: !showQRCode }">
              <div v-if="loginState === 'loading'" class="loading-spinner">
                <span class="spinner"></span>
              </div>
              <img 
                v-else-if="qrcodeUrl" 
                :src="`https://api.qrserver.com/v1/create-qr-code/?size=200x200&data=${encodeURIComponent(qrcodeUrl)}`"
                alt="登录二维码"
                class="qrcode-img"
              />
              
              <!-- 覆盖层（已扫码/过期/错误） -->
              <div v-if="loginState === 'scanned'" class="qrcode-overlay scanned">
                <span class="overlay-icon">✓</span>
                <span class="overlay-text">扫描成功</span>
              </div>
              <div v-else-if="loginState === 'success'" class="qrcode-overlay success">
                <span class="overlay-icon">✓</span>
                <span class="overlay-text">登录成功</span>
              </div>
              <div v-else-if="loginState === 'expired'" class="qrcode-overlay expired">
                <span class="overlay-icon">⏰</span>
                <span class="overlay-text">已过期</span>
              </div>
              <div v-else-if="loginState === 'error'" class="qrcode-overlay error">
                <span class="overlay-icon">!</span>
                <span class="overlay-text">出错了</span>
              </div>
            </div>
            
            <!-- 状态信息 -->
            <div class="status-info">
              <span class="status-icon">{{ stateIcon }}</span>
              <span class="status-text">{{ statusMessage }}</span>
            </div>
            
            <!-- 错误信息 -->
            <div v-if="errorMessage" class="error-info">
              {{ errorMessage }}
            </div>
            
            <!-- 刷新按钮 -->
            <button 
              v-if="showRefreshButton" 
              class="refresh-btn"
              @click="refreshQRCode"
            >
              刷新二维码
            </button>
            
            <!-- 成功信息 -->
            <div v-if="userInfo && loginState === 'success'" class="user-info">
              <img 
                v-if="userInfo.face" 
                :src="userInfo.face" 
                class="user-avatar"
                referrerpolicy="no-referrer"
              />
              <span class="user-name">欢迎，{{ userInfo.uname }}！</span>
            </div>
          </div>
          
          <!-- 底部提示 -->
          <div class="dialog-footer">
            <span class="tip">💡 打开哔哩哔哩 App → 右上角扫一扫</span>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped lang="scss">
.login-overlay {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background: rgba(0, 0, 0, 0.6);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 9999;
  backdrop-filter: blur(4px);
}

.login-dialog {
  background: var(--bg-primary);
  border-radius: 12px;
  border: 1px solid var(--border-color);
  box-shadow: 0 20px 40px rgba(0, 0, 0, 0.3);
  width: 320px;
  overflow: hidden;
  animation: dialogIn 0.2s ease;
}

@keyframes dialogIn {
  from {
    opacity: 0;
    transform: scale(0.95);
  }
  to {
    opacity: 1;
    transform: scale(1);
  }
}

.dialog-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 16px 20px;
  border-bottom: 1px solid var(--border-color);
  background: var(--bg-secondary);
  
  h3 {
    margin: 0;
    font-size: 16px;
    font-weight: 600;
    color: var(--text-primary);
  }
  
  .close-btn {
    width: 24px;
    height: 24px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: transparent;
    border: none;
    color: var(--text-secondary);
    font-size: 14px;
    cursor: pointer;
    border-radius: 4px;
    transition: all 0.15s;
    
    &:hover {
      background: var(--bg-hover);
      color: var(--text-primary);
    }
  }
}

.dialog-content {
  padding: 24px;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 16px;
}

.qrcode-container {
  width: 200px;
  height: 200px;
  background: white;
  border-radius: 8px;
  display: flex;
  align-items: center;
  justify-content: center;
  position: relative;
  overflow: hidden;
  
  &.dimmed {
    .qrcode-img {
      filter: blur(4px);
      opacity: 0.3;
    }
  }
}

.qrcode-img {
  width: 100%;
  height: 100%;
  transition: all 0.3s;
}

.loading-spinner {
  display: flex;
  align-items: center;
  justify-content: center;
}

.spinner {
  width: 32px;
  height: 32px;
  border: 3px solid var(--border-color);
  border-top-color: var(--accent-primary);
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}

.qrcode-overlay {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  background: rgba(0, 0, 0, 0.75);
  
  .overlay-icon {
    width: 48px;
    height: 48px;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 24px;
    color: white;
  }
  
  .overlay-text {
    color: white;
    font-size: 14px;
    font-weight: 500;
  }
  
  &.scanned .overlay-icon {
    background: #22c55e;
  }
  
  &.success .overlay-icon {
    background: #22c55e;
    animation: pulse 0.5s ease;
  }
  
  &.expired .overlay-icon {
    background: #f59e0b;
  }
  
  &.error .overlay-icon {
    background: #ef4444;
  }
}

@keyframes pulse {
  0%, 100% { transform: scale(1); }
  50% { transform: scale(1.1); }
}

.status-info {
  display: flex;
  align-items: center;
  gap: 8px;
  
  .status-icon {
    font-size: 18px;
  }
  
  .status-text {
    font-size: 14px;
    color: var(--text-secondary);
  }
}

.error-info {
  padding: 8px 12px;
  background: rgba(239, 68, 68, 0.1);
  border: 1px solid rgba(239, 68, 68, 0.3);
  border-radius: 6px;
  color: #ef4444;
  font-size: 12px;
  text-align: center;
  max-width: 100%;
  word-break: break-word;
}

.refresh-btn {
  padding: 10px 24px;
  background: var(--accent-primary);
  border: none;
  border-radius: 6px;
  color: white;
  font-size: 14px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.15s;
  
  &:hover {
    opacity: 0.9;
    transform: translateY(-1px);
  }
  
  &:active {
    transform: translateY(0);
  }
}

.user-info {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 16px;
  background: rgba(34, 197, 94, 0.1);
  border: 1px solid rgba(34, 197, 94, 0.3);
  border-radius: 8px;
  
  .user-avatar {
    width: 32px;
    height: 32px;
    border-radius: 50%;
  }
  
  .user-name {
    font-size: 14px;
    font-weight: 500;
    color: #22c55e;
  }
}

.dialog-footer {
  padding: 12px 20px;
  border-top: 1px solid var(--border-color);
  background: var(--bg-secondary);
  
  .tip {
    font-size: 12px;
    color: var(--text-muted);
  }
}

// 过渡动画
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.2s ease;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
</style>

