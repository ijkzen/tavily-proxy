import { useEffect, useState } from 'react'
import { Monitor, Moon, Sun } from 'lucide-react'
import { Button } from '@/components/ui/button'

type Theme = 'light' | 'dark' | 'system'

const NEXT: Record<Theme, Theme> = { light: 'dark', dark: 'system', system: 'light' }

function resolve(t: Theme): boolean {
  return t === 'dark' || (t === 'system' && window.matchMedia('(prefers-color-scheme: dark)').matches)
}

/** 三态主题切换：浅色 → 深色 → 跟随系统，循环。localStorage 持久化，index.html 内联脚本防闪烁。 */
export function ThemeToggle({ className }: { className?: string }) {
  const [theme, setTheme] = useState<Theme>(() => (localStorage.getItem('theme') as Theme) || 'system')

  useEffect(() => {
    document.documentElement.classList.toggle('dark', resolve(theme))
    localStorage.setItem('theme', theme)
  }, [theme])

  useEffect(() => {
    const mq = window.matchMedia('(prefers-color-scheme: dark)')
    const onChange = () => {
      if (theme === 'system') document.documentElement.classList.toggle('dark', mq.matches)
    }
    mq.addEventListener('change', onChange)
    return () => mq.removeEventListener('change', onChange)
  }, [theme])

  const Icon = theme === 'light' ? Sun : theme === 'dark' ? Moon : Monitor
  const label = theme === 'light' ? '切换到深色' : theme === 'dark' ? '跟随系统' : '切换到浅色'

  return (
    <Button
      variant="ghost"
      size="icon"
      className={className}
      title={label}
      aria-label={label}
      onClick={() => setTheme(NEXT[theme])}
    >
      <Icon className="size-4" />
    </Button>
  )
}
