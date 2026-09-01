import { useEffect, useState } from 'react'
import { Monitor, Moon, Sun } from 'lucide-react'
import { Button } from '@/components/ui/button'

type Theme = 'light' | 'dark' | 'system'

const NEXT: Record<Theme, Theme> = { light: 'dark', dark: 'system', system: 'light' }

function resolve(t: Theme): boolean {
  return t === 'dark' || (t === 'system' && window.matchMedia('(prefers-color-scheme: dark)').matches)
}

/** 同步应用主题到 <html>：view transition 回调内需立即生效，不能依赖 React effect。 */
function apply(theme: Theme) {
  document.documentElement.classList.toggle('dark', resolve(theme))
  localStorage.setItem('theme', theme)
}

/** 三态主题切换：浅色 → 深色 → 跟随系统，循环。localStorage 持久化，index.html 内联脚本防闪烁。 */
export function ThemeToggle({ className }: { className?: string }) {
  const [theme, setTheme] = useState<Theme>(() => {
    const t = localStorage.getItem('theme')
    return t === 'light' || t === 'dark' || t === 'system' ? t : 'system'
  })

  // 系统偏好变化时，跟随系统模式实时切换
  useEffect(() => {
    const mq = window.matchMedia('(prefers-color-scheme: dark)')
    const onChange = () => apply(theme)
    mq.addEventListener('change', onChange)
    return () => mq.removeEventListener('change', onChange)
  }, [theme])

  function toggle() {
    const next = NEXT[theme]
    // View Transitions API：主题切换带淡入过渡；不支持时直接应用
    if (document.startViewTransition) {
      document.startViewTransition(() => {
        apply(next)
        setTheme(next)
      })
    } else {
      apply(next)
      setTheme(next)
    }
  }

  const Icon = theme === 'light' ? Sun : theme === 'dark' ? Moon : Monitor
  const label = theme === 'light' ? '切换到深色' : theme === 'dark' ? '跟随系统' : '切换到浅色'

  return (
    <Button
      variant="ghost"
      size="icon"
      className={className}
      title={label}
      aria-label={label}
      onClick={toggle}
    >
      <Icon className="size-4" />
    </Button>
  )
}
