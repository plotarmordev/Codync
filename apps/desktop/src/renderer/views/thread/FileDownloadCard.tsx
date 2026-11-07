import { useState } from 'react'
import { showsAsAudio, showsAsImage, type Entry, type SharedFile } from '@shared/models'
import { useStore } from '../../store/context'
import { Icon } from '../../components/Icon'
import { AnchoredMenu, IconButtonBase, useContextMenu } from '../../components/Overlay'
import { Spinner } from '../../components/Controls'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { AttachmentImage, AttachmentViewer, attachmentSymbol, type ImageSource } from './Attachments'
import { cardActions, cardDetail, cardKeyAction, clickRunsFirstAction, downloadAction } from './file-card'
import type { AudioSnapshot } from './shared-audio'
import { useSharedAudio } from './use-shared-audio'
import './attachments.css'

export function FileDownloadCard({ entry, file }: { entry: Entry; file: SharedFile }) {
  return showsAsAudio(file) ? <AudioFileCard entry={entry} file={file} /> : <FileCard entry={entry} file={file} />
}

/** A voice note's card: its play state (and a spinner while it loads) in place of the file icon. */
function AudioFileCard({ entry, file }: { entry: Entry; file: SharedFile }) {
  const { snapshot, toggle } = useSharedAudio(file, entry.id)
  return <FileCard entry={entry} file={file} audio={snapshot} toggle={toggle} icon={<AudioIcon snapshot={snapshot} toggle={toggle} />} />
}

/**
 * One screen-reader and keyboard stop, as on the phones: the card reads its name and caption, a click, Enter or
 * Space does its first action, and its menu (right-click, Shift-F10, VoiceOver's Show menu) offers all of them. The
 * buttons inside are for the pointer only.
 */
function FileCard({ entry, file, audio, toggle, icon }: { entry: Entry; file: SharedFile; audio?: AudioSnapshot; toggle?: () => void; icon?: React.ReactNode }) {
  const store = useStore()
  const state = store.fileDownloads.states.get(file.id)
  const busy = state?.kind === 'downloading'
  const failed = state?.kind === 'failed' || audio?.phase === 'failed'
  const detail = cardDetail(state, file, audio)
  const [viewing, setViewing] = useState(false)
  const menu = useContextMenu()
  const image: ImageSource = { file, entryId: entry.id }
  const download = () => busy ? store.fileDownloads.cancel(file.id) : void store.fileDownloads.save(store.client, entry.id, file)
  const actions = () => cardActions({ state, audio, toggle, view: showsAsImage(file) ? () => setViewing(true) : undefined, download })
  const onKeyDown = (e: React.KeyboardEvent<HTMLDivElement>) => {
    if (e.target !== e.currentTarget) return
    const action = cardKeyAction(e, menu.open)
    if (!action) return
    e.preventDefault()
    if (action === 'first') actions()[0].action()
    else {
      const { left, bottom } = e.currentTarget.getBoundingClientRect()
      menu.openAt({ x: left, y: bottom })
    }
  }
  return <>
    <div className="file-card" role="button" tabIndex={0} aria-label={`${file.name}, ${detail}`}
      onClick={(e) => clickRunsFirstAction(e.target as Element) && actions()[0].action()} onKeyDown={onKeyDown} onContextMenu={menu.onContextMenu}>
      <VStack gap={12} aria-hidden>
        {showsAsImage(file) ? <button className="file-card-image" tabIndex={-1} onClick={() => setViewing(true)}>
          <AttachmentImage source={image} fit="thumbnail" />
        </button> : null}
        <HStack gap={12}>
          {icon ?? <Icon name={attachmentSymbol(file.name)} size={24} color="var(--secondary)" />}
          <VStack grow>
            <Text as="div" size="subheadline" weight="semibold" truncate title={file.name}>{file.name}</Text>
            <Text as="div" size="caption2" tone={failed ? 'danger' : 'secondary'} className="file-card-detail">{detail}</Text>
            {audio && audio.duration > 0 ? <div className="file-card-progress"><div style={{ width: `${(audio.elapsed / audio.duration) * 100}%` }} /></div> : null}
          </VStack>
          <IconButtonBase title={downloadAction(state)} icon={busy ? 'xmark' : 'arrow.down.to.line'} size={36} tabIndex={-1} onClick={download} />
        </HStack>
      </VStack>
    </div>
    <AnchoredMenu open={menu.open} onClose={menu.close} point={menu.point} items={actions} />
    <AttachmentViewer source={viewing ? image : null} close={() => setViewing(false)} />
  </>
}

/** The play/pause button, or the spinner while the sound comes over, in the file icon's 24 px slot. */
function AudioIcon({ snapshot, toggle }: { snapshot: AudioSnapshot; toggle: () => void }) {
  return <div className="file-card-play">
    {snapshot.phase === 'loading'
      ? <Spinner size={18} />
      : <IconButtonBase title={snapshot.phase === 'playing' ? 'Pause' : 'Play'} icon={snapshot.phase === 'playing' ? 'pause.circle' : 'play.circle'} size={36} tabIndex={-1} onClick={toggle} />}
  </div>
}
