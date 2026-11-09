import { DropdownMenu } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { usePref } from '../../lib/prefs'
import { useApp, useStore } from '../../store/context'
import { dictationEngine, dictationEngineOptions, localEngineName } from '../settings/voice-settings'

/** Beside the mic, in its capsule: which speech dictation uses — on-device, a provider on the user's own key, or the computer's speech server. */
export function DictationEngineMenu() {
  const app = useApp()
  const store = useStore()
  const [engine, setEngine] = usePref(dictationEngine)
  return (
    <DropdownMenu
      className="press composer-engine"
      title="Dictation engine"
      items={() => dictationEngineOptions(localEngineName(store === app.local, store.hostName)).map((e) => ({ title: e.label, selected: e.id === engine, action: () => setEngine(e.id) }))}
    >
      <Icon name="chevron.up" size={10} weight="semibold" />
    </DropdownMenu>
  )
}
