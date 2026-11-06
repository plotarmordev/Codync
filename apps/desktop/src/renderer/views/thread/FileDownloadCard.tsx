import { useState } from 'react'
import { showsAsAudio, showsAsImage, type Entry, type SharedFile } from '@shared/models'
import { useStore } from '../../store/context'
import type { DownloadState } from '../../store/file-downloads'
import { Icon } from '../../components/Icon'
import { IconButtonBase } from '../../components/Overlay'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { AttachmentImage, AttachmentViewer, attachmentSymbol, byteCount, type ImageSource } from './Attachments'
import { useSharedAudio } from './use-shared-audio'
import './attachments.css'

/** What the download button does now (also the Library's file rows). */
export function downloadAction(state: DownloadState | undefined) {
  return state?.kind === 'downloading' ? 'Cancel download' : state?.kind === 'failed' ? 'Retry download' : state?.kind === 'saved' ? 'Save again' : 'Download'
}

/** The line under a shared file's name: its size, the download's progress, or what went wrong. */
export function downloadDetail(state: DownloadState | undefined, file: SharedFile) {
  switch (state?.kind) {
    case 'downloading': return `${byteCount(state.received)} of ${byteCount(file.size)}`
    case 'failed': return state.message
    case 'saved': return `${byteCount(file.size)} · Saved`
    default: return byteCount(file.size)
  }
}

export function FileDownloadCard({ entry, file }: { entry: Entry; file: SharedFile }) {
  const store = useStore()
  const state = store.fileDownloads.states.get(file.id)
  const busy = state?.kind === 'downloading'
  const [action, detail] = [downloadAction(state), downloadDetail(state, file)]
  const [viewing, setViewing] = useState(false)
  const image: ImageSource = { file, entryId: entry.id }
  return <VStack gap={12} className="file-card">
    {showsAsImage(file) ? <>
      <button className="file-card-image" aria-label={file.name} onClick={() => setViewing(true)}>
        <AttachmentImage source={image} fit="thumbnail" />
      </button>
      <AttachmentViewer source={viewing ? image : null} close={() => setViewing(false)} />
    </> : null}
    <HStack gap={12}>
      {showsAsAudio(file) ? <AudioButton file={file} entryId={entry.id} /> : <Icon name={attachmentSymbol(file.name)} size={24} color="var(--secondary)" />}
      <VStack grow>
        <Text as="div" size="subheadline" weight="semibold" truncate title={file.name}>{file.name}</Text>
        <Text as="div" size="caption2" tone={state?.kind === 'failed' ? 'danger' : 'secondary'} role={state?.kind === 'failed' ? 'alert' : 'status'} className="file-card-detail">{detail}</Text>
      </VStack>
      <IconButtonBase title={action} icon={busy ? 'xmark' : 'arrow.down.to.line'} size={36} onClick={() => busy ? store.fileDownloads.cancel(file.id) : void store.fileDownloads.save(store.client, entry.id, file)} />
    </HStack>
  </VStack>
}

/** A voice note's play/pause, in place of its file icon. */
function AudioButton({ file, entryId }: { file: SharedFile; entryId: string }) {
  const { playing, toggle } = useSharedAudio(file, entryId)
  return <IconButtonBase title={`${playing ? 'Pause' : 'Play'} ${file.name}`} icon={playing ? 'pause.circle' : 'play.circle'} size={36} onClick={toggle} />
}
