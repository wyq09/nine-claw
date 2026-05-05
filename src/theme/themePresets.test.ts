import { describe, expect, it } from 'vitest'
import { THEME_OPTIONS, THEME_PRESETS, THEME_VARIABLE_KEYS } from './themePresets'

describe('themePresets', () => {
  it('exposes the shrimp tide preset in theme options', () => {
    expect(THEME_OPTIONS).toContainEqual({
      value: 'shrimp_tide',
      label: '虾游·潮汐间',
    })
  })

  it('maps shrimp tide core tokens to the tidal palette', () => {
    const preset = THEME_PRESETS.shrimp_tide

    expect(preset.colorScheme).toBe('dark')
    expect(preset.variables['--app-bg']).toBe('#0b1120')
    expect(preset.variables['--sidebar-bg']).toBe('#0f1a2e')
    expect(preset.variables['--panel']).toBe('#162236')
    expect(preset.variables['--blue']).toBe('#e87d65')
    expect(preset.variables['--text']).toBe('#e8e4dc')
    expect(preset.variables['--text-muted']).toBe('#9aa3b2')
    expect(preset.variables['--chrome-accent']).toBe('#c8d6e0')
    expect(preset.variables['--control-border-focus']).toBe('#7ee8d0')
  })

  it('includes shrimp tide variables in the merged theme key list', () => {
    expect(THEME_VARIABLE_KEYS).toContain('--history-menu-hover-bg')
    expect(THEME_VARIABLE_KEYS).toContain('--runtime-banner-success-bg')
    expect(THEME_VARIABLE_KEYS).toContain('--user-bubble-bg')
  })
})
