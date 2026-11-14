import { useState } from 'react'
import { filePreview } from '@shared/file-preview'
import { showsAsImage, type Entry, type SharedFile } from '@shared/models'
import { useStore } from '../../store/context'
import { Icon } from '../../components/Icon'
import { AnchoredMenu, useContextMenu, type MenuItem } from '../../components/Menu'
import { IconButtonBase } from '../../components/Overlay'
import { Spinner } from '../../components/Controls'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { AttachmentImage, AttachmentViewer, attachmentSymbol, type ImageSource } from './Attachments'
import { cardActions, cardDetail, cardKeyAction, clickRunsFirstAction, downloadAction, downloadDetail } from './file-card'
import { ImageContextMenu } from './ImageContextMenu'
import { imageCopyItems, type MessageMenu } from './use-image-actions'
import { analytics } from '../../store/analytics'
import { imageActionEvent } from '../../lib/image-analytics.ts'
import { FileViewer } from './FileViewer'
import type { AudioSnapshot } from './shared-audio'
import { useSharedAudio } from './use-shared-audio'
import { useTr } from '../../lib/use-language'
import './attachments.css'

export function FileDownloadCard({ entry, file, menu }: { entry: Entry; file: SharedFile; menu: MessageMenu }) {
  return file.playsInChat ? <VoiceNotePlayer entry={entry} file={file} menu={menu} named /> : <FileCard entry={entry} file={file} menu={menu} />
}

/**
 * A voice note's player, the same for one inside a message (at the top of its bubble, the text follows) and one sent
 * on its own (`named`: a bubble of its own, the file's name under the row): play, the bar and the clock from the first
 * frame (the host reads the length). Download is in the message's menu, not a button of its own.
 */
export function VoiceNotePlayer({ entry, file, menu, named = false }: { entry: Entry; file: SharedFile; menu: MessageMenu; named?: boolean }) {
  const tr = useTr()
  const { snapshot, toggle, seek } = useSharedAudio(file, entry.id)
  const stop = useFileStop(entry, file, menu, snapshot, toggle, seek)
  return <>
    <div className={named ? 'voice-note bubble agent' : 'voice-note'} {...stop.props} aria-label={[named ? file.name : tr('Voice note'), stop.detail].filter(Boolean).join(', ')}>
      <VStack gap={4} aria-hidden>
        <HStack gap={10}>
          <AudioIcon snapshot={snapshot} toggle={toggle} />
          {stop.failed ? null : <AudioProgress snapshot={snapshot} seek={seek} />}
          {stop.detail ? <Text as="div" size="caption2" tone={stop.failed ? 'danger' : 'secondary'} className="file-card-detail" grow={stop.failed}>{stop.detail}</Text> : null}
        </HStack>
        {named ? <Text as="div" size="subheadline" weight="semibold" truncate title={file.name}>{file.name}</Text> : null}
      </VStack>
    </div>
    {stop.overlays}
  </>
}

/**
 * One screen-reader and keyboard stop, as on the phones: the card reads its name and caption, a click, Enter or
 * Space does its first action, the arrow keys move a voice note's playhead, and its menu (right-click, Shift-F10,
 * VoiceOver's Show menu) offers all of them, then the message's own (Reply, Copy, reactions).
 * The buttons and bar inside are for the pointer only.
 */
function FileCard({ entry, file, menu }: { entry: Entry; file: SharedFile; menu: MessageMenu }) {
  const stop = useFileStop(entry, file, menu)
  return <>
    <div className="file-card" {...stop.props} aria-label={`${file.name}, ${stop.detail}`}>
      <VStack gap={12} aria-hidden>
        {showsAsImage(file) ? <ImageContextMenu source={stop.image} menu={menu} className="file-card-image-menu">
          <button className="file-card-image" tabIndex={-1} onClick={stop.view}>
            <AttachmentImage source={stop.image} fit="thumbnail" />
          </button>
        </ImageContextMenu> : null}
        <HStack gap={12}>
          <Icon name={attachmentSymbol(file.name)} size={24} color="var(--secondary)" />
          <VStack grow>
            <Text as="div" size="subheadline" weight="semibold" truncate title={file.name}>{file.name}</Text>
            <Text as="div" size="caption2" tone={stop.failed ? 'danger' : 'secondary'} className="file-card-detail">{stop.detail}</Text>
          </VStack>
          <IconButtonBase title={downloadAction(stop.state)} icon={stop.busy ? 'xmark' : 'arrow.down.to.line'} size={36} tabIndex={-1} onClick={stop.download} />
        </HStack>
      </VStack>
    </div>
    {stop.overlays}
  </>
}

/** Saves a shared file (or stops that) beside the close button of its full-size picture; reported once the save ends, a cancel says nothing. */
function ViewerDownloadButton({ entry, file }: { entry: Entry; file: SharedFile }) {
  const store = useStore()
  const state = store.fileDownloads.states.get(file.id)
  const busy = state?.kind === 'downloading'
  const start = async () => {
    if (busy) return store.fileDownloads.cancel(file.id)
    const outcome = await store.fileDownloads.save(store.client, entry.id, file)
    if (outcome !== 'cancelled') analytics.track(imageActionEvent('download', 'viewer', outcome === 'failed' ? 'transfer' : undefined))
  }
  return <IconButtonBase title={downloadAction(state)} icon={busy ? 'xmark' : 'arrow.down.to.line'} size={28} onClick={() => void start()} />
}

/** A shared file's keyboard and screen-reader stop: its caption, its actions with their menu, and the picture or file viewer. */
function useFileStop(entry: Entry, file: SharedFile, menu: MessageMenu, audio?: AudioSnapshot, toggle?: () => void, seek?: (seconds: number) => void) {
  const store = useStore()
  const tr = useTr()
  const state = store.fileDownloads.states.get(file.id)
  const busy = state?.kind === 'downloading'
  const [viewing, setViewing] = useState(false)
  const [reading, setReading] = useState(false)
  const context = useContextMenu()
  const image: ImageSource = { file, entryId: entry.id }
  const view = () => setViewing(true)
  const download = () => store.fileDownloads.toggle(store.client, entry.id, file)
  const read = () => setReading(true)
  const actions = () => cardActions({ state, audio, toggle, view: showsAsImage(file) ? view : filePreview(file) ? read : undefined, download })
  const onKeyDown = (e: React.KeyboardEvent<HTMLDivElement>) => {
    if (e.target !== e.currentTarget) return
    const action = cardKeyAction(e, context.open, seek ? audio : undefined)
    if (!action) return
    e.preventDefault()
    if (action.kind === 'seek') seek?.(action.seconds)
    else if (action.kind === 'first') actions()[0].action()
    else {
      const { left, bottom } = e.currentTarget.getBoundingClientRect()
      context.openAt({ x: left, y: bottom })
    }
  }
  return {
    state,
    busy,
    failed: state?.kind === 'failed' || audio?.phase === 'failed',
    detail: cardDetail(state, file, audio, tr),
    image,
    view,
    download,
    props: {
      role: 'button',
      tabIndex: 0,
      onClick: (e: React.MouseEvent) => clickRunsFirstAction(e.target as Element) && actions()[0].action(),
      onKeyDown,
      onContextMenu: context.onContextMenu,
    },
    overlays: <>
      <AnchoredMenu open={context.open} onClose={context.close} point={context.point} reactions={menu.reactions}
        items={() => [...actions(), ...withDivider([...(showsAsImage(file) ? imageCopyItems(store, image) : []), ...menu.items()])]} />
      <AttachmentViewer
        source={viewing ? image : null}
        close={() => setViewing(false)}
        menu={menu}
        actions={<ViewerDownloadButton entry={entry} file={file} />}
        note={state ? <Text as="div" size="caption2" tone={state.kind === 'failed' ? 'danger' : 'secondary'} role={state.kind === 'failed' ? 'alert' : 'status'} className="attachment-viewer-note">{downloadDetail(state, file)}</Text> : undefined}
      />
      <FileViewer entryId={entry.id} file={reading ? file : null} close={() => setReading(false)} />
    </>,
  }
}

/** The card menu's items after its own: the first one starts a new group. */
function withDivider(items: MenuItem[]) {
  return items.map((item, i) => i === 0 ? { ...item, divider: true } : item)
}

/** The play/pause glyph, filled while the pointer is on it, or the spinner while the sound comes over; no tile behind it in any state. */
function AudioIcon({ snapshot, toggle }: { snapshot: AudioSnapshot; toggle: () => void }) {
  const [hovered, setHovered] = useState(false)
  const glyph = `${snapshot.phase === 'playing' ? 'pause' : 'play'}.circle${hovered ? '.fill' : ''}`
  return <div className="voice-note-slot" onMouseEnter={() => setHovered(true)} onMouseLeave={() => setHovered(false)}>
    {snapshot.phase === 'loading'
      ? <Spinner size={18} />
      : <IconButtonBase title={snapshot.phase === 'playing' ? 'Pause' : 'Play'} icon={glyph} size={36} tabIndex={-1} className="voice-note-glyph" onClick={toggle} />}
  </div>
}

/** How much of the voice note has played, empty until it plays; click or drag it to move a loaded sound there. */
function AudioProgress({ snapshot, seek }: { snapshot: AudioSnapshot; seek: (seconds: number) => void }) {
  const played = snapshot.duration > 0 ? snapshot.elapsed / snapshot.duration : 0
  return <input type="range" className="voice-note-progress" tabIndex={-1} min={0} max={snapshot.duration || 1} step="any"
    value={snapshot.elapsed} style={{ '--played': played } as React.CSSProperties}
    onChange={(e) => seek(Number(e.target.value))} />
}
