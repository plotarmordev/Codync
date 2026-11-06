import { useState } from 'react'
import { showsAsAudio, showsAsImage, type Entry, type SharedFile } from '@shared/models'
import { useStore } from '../../store/context'
import { Icon } from '../../components/Icon'
import { IconButtonBase } from '../../components/Overlay'
import { Spinner } from '../../components/Controls'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { AttachmentImage, AttachmentViewer, attachmentSymbol, type ImageSource } from './Attachments'
import { cardDetail, downloadAction } from './file-card'
import type { AudioSnapshot } from './shared-audio'
import { useSharedAudio } from './use-shared-audio'
import './attachments.css'

export function FileDownloadCard({ entry, file }: { entry: Entry; file: SharedFile }) {
  return showsAsAudio(file) ? <AudioFileCard entry={entry} file={file} /> : <FileCard entry={entry} file={file} />
}

/** A voice note's card: its play state (and a spinner while it loads) in place of the file icon. */
function AudioFileCard({ entry, file }: { entry: Entry; file: SharedFile }) {
  const { snapshot, toggle } = useSharedAudio(file, entry.id)
  return <FileCard entry={entry} file={file} audio={snapshot} icon={<AudioIcon snapshot={snapshot} toggle={toggle} />} />
}

function FileCard({ entry, file, audio, icon }: { entry: Entry; file: SharedFile; audio?: AudioSnapshot; icon?: React.ReactNode }) {
  const store = useStore()
  const state = store.fileDownloads.states.get(file.id)
  const busy = state?.kind === 'downloading'
  const failed = state?.kind === 'failed' || audio?.phase === 'failed'
  const [action, detail] = [downloadAction(state), cardDetail(state, file, audio)]
  const [viewing, setViewing] = useState(false)
  const image: ImageSource = { file, entryId: entry.id }
  return <VStack gap={12} className="file-card" role="group" aria-label={`${file.name}, ${detail}`}>
    {showsAsImage(file) ? <>
      <button className="file-card-image" aria-label="View" onClick={() => setViewing(true)}>
        <AttachmentImage source={image} fit="thumbnail" />
      </button>
      <AttachmentViewer source={viewing ? image : null} close={() => setViewing(false)} />
    </> : null}
    <HStack gap={12}>
      {icon ?? <Icon name={attachmentSymbol(file.name)} size={24} color="var(--secondary)" />}
      <VStack grow>
        <Text as="div" size="subheadline" weight="semibold" truncate title={file.name}>{file.name}</Text>
        <Text as="div" size="caption2" tone={failed ? 'danger' : 'secondary'} role={failed ? 'alert' : 'status'} className="file-card-detail">{detail}</Text>
        {audio && audio.duration > 0 ? <div className="file-card-progress"><div style={{ width: `${(audio.elapsed / audio.duration) * 100}%` }} /></div> : null}
      </VStack>
      <IconButtonBase title={action} icon={busy ? 'xmark' : 'arrow.down.to.line'} size={36} onClick={() => busy ? store.fileDownloads.cancel(file.id) : void store.fileDownloads.save(store.client, entry.id, file)} />
    </HStack>
  </VStack>
}

/** The play/pause button, or the spinner while the sound comes over, in the file icon's 24 px slot. */
function AudioIcon({ snapshot, toggle }: { snapshot: AudioSnapshot; toggle: () => void }) {
  return <div className="file-card-play">
    {snapshot.phase === 'loading'
      ? <Spinner size={18} />
      : <IconButtonBase title={snapshot.phase === 'playing' ? 'Pause' : 'Play'} icon={snapshot.phase === 'playing' ? 'pause.circle' : 'play.circle'} size={36} onClick={toggle} />}
  </div>
}
