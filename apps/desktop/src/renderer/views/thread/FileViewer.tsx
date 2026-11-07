import { useEffect, useState } from 'react'
import { filePreview, opensInline, type FilePreview, type SharedFile } from '@shared/models'
import { IconButton } from '../../components/Controls'
import { ModalHeader, Sheet, SheetPage } from '../../components/Overlay'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { useStore } from '../../store/context'
import { downloadAction, downloadDetail } from './Attachments'
import { MarkdownText } from './Markdown'
import './attachments.css'

/** A shared Markdown, text or PDF file open inside the app, from its card or its Library row. */
export function FileViewer({ entryId, file, close }: { entryId: string; file: SharedFile | null; close: () => void }) {
  const store = useStore()
  const state = file ? store.fileDownloads.states.get(file.id) : undefined
  const busy = state?.kind === 'downloading'
  const preview = file && filePreview(file)
  return (
    <Sheet open={file !== null} onClose={close} width={920} height={720}>
      {file && preview ? (
        <SheetPage>
          <ModalHeader
            title={file.name}
            subtitle={state ? downloadDetail(state, file) : undefined}
            trailing={
              <IconButton
                title={downloadAction(state)}
                icon={busy ? 'xmark' : 'arrow.down.to.line'}
                onClick={() => (busy ? store.fileDownloads.cancel(file.id) : void store.fileDownloads.save(store.client, entryId, file))}
              />
            }
          />
          <FileBody key={file.id} entryId={entryId} file={file} preview={preview} />
        </SheetPage>
      ) : null}
    </Sheet>
  )
}

type Opened =
  | { kind: 'loading' }
  | { kind: 'big' }
  | { kind: 'notText' }
  | { kind: 'failed' }
  | { kind: 'text'; text: string }
  | { kind: 'pdf'; url: string }

function FileBody({ entryId, file, preview }: { entryId: string; file: SharedFile; preview: FilePreview }) {
  const store = useStore()
  const tr = useTr()
  const [opened, setOpened] = useState<Opened>(opensInline(file) ? { kind: 'loading' } : { kind: 'big' })
  useEffect(() => {
    if (!opensInline(file)) return
    let live = true
    let url: string | null = null
    void store.sharedFileData(file, entryId).then((data) => {
      if (!live) return
      if (!data) return setOpened({ kind: 'failed' })
      if (preview.kind === 'pdf') {
        url = URL.createObjectURL(new Blob([data as BlobPart], { type: 'application/pdf' }))
        return setOpened({ kind: 'pdf', url })
      }
      try {
        setOpened({ kind: 'text', text: new TextDecoder('utf-8', { fatal: true }).decode(data) })
      } catch {
        setOpened({ kind: 'notText' })
      }
    }, () => live && setOpened({ kind: 'failed' }))
    return () => {
      live = false
      if (url) URL.revokeObjectURL(url)
    }
    // A file's id names its bytes for good; `file` is a new object every render.
  }, [store, entryId, file.id])
  const note = (text: string) => (
    <div className="file-viewer">
      <Text size="callout" tone="secondary">{text}</Text>
    </div>
  )
  switch (opened.kind) {
    case 'loading':
      return note(tr('Loading…'))
    case 'big':
      return note(tr('This file is too big to open here (over {limit}). Download it to open it in another app.').replace('{limit}', byteLimit(preview.maxBytes)))
    case 'notText':
      return note(tr('This file isn\'t UTF-8 text. Download it to open it in another app.'))
    case 'failed':
      return note(tr('Couldn\'t open the file.'))
    case 'pdf':
      return <iframe className="file-viewer-pdf" title={file.name} src={opened.url} />
    case 'text':
      return preview.kind === 'markdown' ? (
        <div className="file-viewer-scroll"><MarkdownText text={opened.text} /></div>
      ) : (
        <div className="file-viewer-scroll"><pre className="file-viewer-text selectable">{opened.text}</pre></div>
      )
  }
}

/** A kind's limit as people read it: "512 KB", "20 MB". */
const byteLimit = (bytes: number) => (bytes >= 1024 * 1024 ? `${bytes / (1024 * 1024)} MB` : `${bytes / 1024} KB`)
