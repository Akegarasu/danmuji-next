const DEBUG_STORAGE_KEY = 'danmuji:debug-logs'

type LogMethod = (...args: unknown[]) => void

export interface ScopedLogger {
  debug: LogMethod
  info: LogMethod
  warn: LogMethod
  error: LogMethod
}

const truthyValues = new Set(['1', 'true', 'yes', 'on'])

const isTruthy = (value: string | undefined | null) =>
  value !== undefined && value !== null && truthyValues.has(value.toLowerCase())

export const isDebugLogEnabled = () => {
  if (isTruthy(import.meta.env.VITE_DEBUG_LOGS)) return true

  try {
    return isTruthy(window.localStorage.getItem(DEBUG_STORAGE_KEY))
  } catch {
    return false
  }
}

export const setDebugLogEnabled = (enabled: boolean) => {
  try {
    if (enabled) {
      window.localStorage.setItem(DEBUG_STORAGE_KEY, '1')
    } else {
      window.localStorage.removeItem(DEBUG_STORAGE_KEY)
    }
  } catch {
    // Ignore storage failures; logging should never affect app behavior.
  }
}

export const createLogger = (scope: string): ScopedLogger => {
  const prefix = `[${scope}]`

  return {
    debug: (...args) => {
      if (isDebugLogEnabled()) {
        console.debug(prefix, ...args)
      }
    },
    info: (...args) => console.info(prefix, ...args),
    warn: (...args) => console.warn(prefix, ...args),
    error: (...args) => console.error(prefix, ...args)
  }
}

declare global {
  interface Window {
    danmujiDebug?: {
      enable: () => void
      disable: () => void
      isEnabled: () => boolean
      storageKey: string
    }
  }
}

if (typeof window !== 'undefined') {
  window.danmujiDebug = {
    enable: () => {
      setDebugLogEnabled(true)
      console.info('[Logger] Debug logs enabled')
    },
    disable: () => {
      setDebugLogEnabled(false)
      console.info('[Logger] Debug logs disabled')
    },
    isEnabled: isDebugLogEnabled,
    storageKey: DEBUG_STORAGE_KEY
  }
}
