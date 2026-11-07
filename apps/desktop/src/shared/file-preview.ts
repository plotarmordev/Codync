import type { SharedFile } from './models.ts'

/** A shared file the apps open inside their viewer, and the bytes it fetches at most. */
export interface FilePreview {
  kind: 'markdown' | 'text' | 'pdf' | 'video'
  maxBytes: number
}

const MB = 1024 * 1024

const FILE_PREVIEWS = new Map<string, FilePreview>()
for (const [kind, maxBytes, exts] of [
  ['markdown', 10 * MB, ['md', 'markdown']],
  ['text', 10 * MB, ['txt', 'log', 'csv', 'tsv', 'json', 'jsonl', 'xml', 'yaml', 'yml', 'toml', 'ini', 'swift', 'kt', 'kts', 'java', 'js', 'jsx', 'ts', 'tsx', 'py', 'rb', 'go', 'rs', 'c', 'h', 'cpp', 'hpp', 'cs', 'sh', 'sql', 'html', 'css']],
  ['pdf', 25 * MB, ['pdf']],
  ['video', 25 * MB, ['mp4', 'mov', 'm4v', 'webm']],
] as [FilePreview['kind'], number, string[]][]) {
  for (const ext of exts) FILE_PREVIEWS.set(ext, { kind, maxBytes })
}

/** A shared Markdown, text, PDF or video file the viewer can show, whatever its size (too big opens a note instead).
 * A name without a dot has no extension, as on the phones. */
export const filePreview = (file: SharedFile): FilePreview | null => {
  const dot = file.name.lastIndexOf('.')
  return dot < 0 ? null : FILE_PREVIEWS.get(file.name.slice(dot + 1).toLowerCase()) ?? null
}

/** A shared file small enough to open inside the app. */
export const opensInline = (file: SharedFile) => {
  const preview = filePreview(file)
  return preview !== null && file.size <= preview.maxBytes
}

const EBML = [0x1a, 0x45, 0xdf, 0xa3]
const QUICKTIME_BOXES = ['ftyp', 'moov', 'mdat', 'wide']

/** Whether `data` starts like a video container, whatever the file is called: an ISO media file (MP4, MOV, M4V) with
 * its `ftyp` box or an older QuickTime file's first `moov`/`mdat`/`wide` atom, or Matroska/WebM. */
export const isVideoData = (data: Uint8Array) =>
  data.length >= 8 && (EBML.every((byte, i) => data[i] === byte) || QUICKTIME_BOXES.includes(String.fromCharCode(...data.subarray(4, 8))))

/** A kind's limit as people read it: "10 MB", "25 MB". */
export const byteLimit = (bytes: number) => (bytes >= MB ? `${bytes / MB} MB` : `${bytes / 1024} KB`)
