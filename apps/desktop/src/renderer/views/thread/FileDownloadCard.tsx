import { useState } from 'react'
import { filePreview, showsAsAudio, showsAsImage, type Entry, type SharedFile } from '@shared/models'
import { useTr } from '../../lib/use-language'
import { useStore } from '../../store/context'
import { Icon } from '../../components/Icon'
import { IconButtonBase } from '../../components/Overlay'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { AttachmentImage, AttachmentViewer, attachmentSymbol, downloadAction, downloadDetail, type ImageSource } from './Attachments'
import { FileViewer } from './FileViewer'
import { useSharedAudio } from './use-shared-audio'
import './attachments.css'

export function FileDownloadCard({ entry, file }: { entry: Entry; file: SharedFile }) {
  const store = useStore()
  const tr = useTr()
  const state = store.fileDownloads.states.get(file.id)
  const busy = state?.kind === 'downloading'
  const [action, detail] = [downloadAction(state), downloadDetail(state, file)]
  const [viewing, setViewing] = useState(false)
  const [opening, setOpening] = useState(false)
  const image: ImageSource = { file, entryId: entry.id }
  const name = (
    <VStack grow>
      <Text as="div" size="subheadline" weight="semibold" truncate title={file.name}>{file.name}</Text>
      <Text as="div" size="caption2" tone={state?.kind === 'failed' ? 'danger' : 'secondary'} role={state?.kind === 'failed' ? 'alert' : 'status'} className="file-card-detail">{detail}</Text>
    </VStack>
  )
  return <VStack gap={12} className="file-card">
    {showsAsImage(file) ? <>
      <button className="file-card-image" aria-label={file.name} onClick={() => setViewing(true)}>
        <AttachmentImage source={image} fit="thumbnail" />
      </button>
      <AttachmentViewer source={viewing ? image : null} close={() => setViewing(false)} />
    </> : null}
    <HStack gap={12}>
      {filePreview(file) ? (
        <button className="file-card-open" aria-label={tr('Open {name}').replace('{name}', file.name)} onClick={() => setOpening(true)}>
          <Icon name={attachmentSymbol(file.name)} size={24} color="var(--secondary)" />
          {name}
        </button>
      ) : (
        <>
          {showsAsAudio(file) ? <AudioButton file={file} entryId={entry.id} /> : <Icon name={attachmentSymbol(file.name)} size={24} color="var(--secondary)" />}
          {name}
        </>
      )}
      <IconButtonBase title={action} icon={busy ? 'xmark' : 'arrow.down.to.line'} size={36} onClick={() => busy ? store.fileDownloads.cancel(file.id) : void store.fileDownloads.save(store.client, entry.id, file)} />
    </HStack>
    <FileViewer entryId={entry.id} file={opening ? file : null} close={() => setOpening(false)} />
  </VStack>
}

/** A voice note's play/pause, in place of its file icon. */
function AudioButton({ file, entryId }: { file: SharedFile; entryId: string }) {
  const { playing, toggle } = useSharedAudio(file, entryId)
  return <IconButtonBase title={`${playing ? 'Pause' : 'Play'} ${file.name}`} icon={playing ? 'pause.circle' : 'play.circle'} size={36} onClick={toggle} />
}
