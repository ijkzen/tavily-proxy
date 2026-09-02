/**
 * 极简 Markdown → HTML 渲染（明细弹窗的 extract 内容用）。
 * 只覆盖标题 / 列表 / 代码块 / 引用 / 粗斜体 / 链接 / 段落换行，
 * 足够展示上游抓取的正文；不引入 marked，避免依赖膨胀。
 * 输入来自自己的上游响应，非用户自由输入，XSS 面可控。
 */

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
}

/** 行内：`code`、**粗**、*斜*、[link](url) */
function inline(s: string): string {
  return s
    .replace(/`([^`]+)`/g, '<code>$1</code>')
    .replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
    .replace(/\*([^*]+)\*/g, '<em>$1</em>')
    .replace(/\[([^\]]+)\]\(([^)]+)\)/g, '<a href="$2" target="_blank" rel="noreferrer">$1</a>')
}

export function renderMarkdown(src: string): string {
  const lines = src.replace(/\r\n/g, '\n').split('\n')
  const out: string[] = []
  let inCode = false
  let codeBuf: string[] = []
  let listBuf: string[] = []

  const flushList = () => {
    if (listBuf.length) {
      out.push(`<ul>${listBuf.map((li) => `<li>${li}</li>`).join('')}</ul>`)
      listBuf = []
    }
  }

  for (const raw of lines) {
    const line = raw.trimEnd()

    // 代码块
    if (line.startsWith('```')) {
      if (inCode) {
        out.push(`<pre><code>${escapeHtml(codeBuf.join('\n'))}</code></pre>`)
        codeBuf = []
        inCode = false
      } else {
        flushList()
        inCode = true
      }
      continue
    }
    if (inCode) {
      codeBuf.push(line)
      continue
    }

    // 空行 → 段落分隔
    if (line.trim() === '') {
      flushList()
      out.push('')
      continue
    }

    // 标题
    const h = line.match(/^(#{1,6})\s+(.*)$/)
    if (h) {
      flushList()
      const level = h[1].length
      out.push(`<h${level}>${inline(escapeHtml(h[2]))}</h${level}>`)
      continue
    }
    // 列表项
    const li = line.match(/^\s*[-*+]\s+(.*)$/) || line.match(/^\s*\d+\.\s+(.*)$/)
    if (li) {
      listBuf.push(inline(escapeHtml(li[1])))
      continue
    }
    // 引用
    if (line.startsWith('> ')) {
      flushList()
      out.push(`<blockquote>${inline(escapeHtml(line.slice(2)))}</blockquote>`)
      continue
    }

    flushList()
    out.push(`<p>${inline(escapeHtml(line))}</p>`)
  }
  flushList()
  if (inCode) out.push(`<pre><code>${escapeHtml(codeBuf.join('\n'))}</code></pre>`)

  return out.join('\n')
}
