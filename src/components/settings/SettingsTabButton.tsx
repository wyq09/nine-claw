import { AppIcon, type IconName } from '../AppIcon'

type SettingsTabButtonProps = {
  active: boolean
  icon: IconName
  label: string
  onClick: () => void
}

/** 设置弹窗左侧导航的标签按钮。 */
export function SettingsTabButton({ active, icon, label, onClick }: SettingsTabButtonProps) {
  return (
    <button type="button" className={`settings-tab-button ${active ? 'active' : ''}`} onClick={onClick}>
      <AppIcon name={icon} size={20} />
      <span>{label}</span>
    </button>
  )
}
