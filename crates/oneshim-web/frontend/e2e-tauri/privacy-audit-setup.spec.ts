// e2e-tauri/privacy-audit-setup.spec.ts
import { invokeIpc } from './helpers.js'

describe('J2: Privacy & Settings Persistence', () => {
  /**
   * @tc_id T110
   * @risk_id SEC-001
   * @journey J2 (Privacy-First Setup & Audit)
   * @persona P2 (Privacy-Conscious Enterprise Admin)
   * @priority P0
   * @tauri_only_reason IPC update_setting writes config JSON to disk
   */
  it('T110: Settings write persists PII filter level after reload', async () => {
    // 현재 설정 저장
    const before = await invokeIpc<Record<string, any>>('get_settings')
    const originalLevel = before?.privacy?.pii_filter_level

    // PII filter를 Strict로 변경
    const patch = { privacy: { pii_filter_level: 'Strict' } }
    await invokeIpc('update_setting', { config_json: JSON.stringify(patch) })

    // 페이지 리로드 (디스크에서 다시 읽기)
    await browser.url('tauri://localhost/')
    await browser.pause(2000) // config reload 대기

    // 변경 사항이 유지되는지 확인
    const after = await invokeIpc<Record<string, any>>('get_settings')
    expect(after.privacy.pii_filter_level).toBe('Strict')

    // 원래 값 복원
    if (originalLevel && originalLevel !== 'Strict') {
      const restore = { privacy: { pii_filter_level: originalLevel } }
      await invokeIpc('update_setting', { config_json: JSON.stringify(restore) })
    }
  })

  /**
   * @tc_id T111
   * @risk_id SEC-002
   * @journey J2 (Privacy-First Setup & Audit)
   * @persona P2 (Privacy-Conscious Enterprise Admin)
   * @priority P0
   * @tauri_only_reason IPC update_setting writes config JSON to disk
   */
  it('T111: Settings write persists capture_enabled toggle after reload', async () => {
    const before = await invokeIpc<Record<string, any>>('get_settings')
    const originalEnabled = before?.capture?.capture_enabled

    // capture 비활성화
    const patch = { capture: { capture_enabled: false } }
    await invokeIpc('update_setting', { config_json: JSON.stringify(patch) })

    await browser.url('tauri://localhost/')
    await browser.pause(2000)

    const after = await invokeIpc<Record<string, any>>('get_settings')
    expect(after.capture.capture_enabled).toBe(false)

    // 원래 값 복원
    if (originalEnabled !== false) {
      const restore = { capture: { capture_enabled: true } }
      await invokeIpc('update_setting', { config_json: JSON.stringify(restore) })
    }
  })

  /**
   * @tc_id T113
   * @risk_id PRIV-002
   * @journey J2 (Privacy-First Setup & Audit)
   * @persona P2 (Privacy-Conscious Enterprise Admin)
   * @priority P0
   * @tauri_only_reason ConfirmModal exists in real Tauri app (Privacy.tsx:14-43)
   */
  it('T113: Data deletion requires confirmation dialog', async () => {
    // Privacy 페이지로 이동
    await browser.url('tauri://localhost/privacy')
    await browser.pause(2000)

    // "Delete All Data" 버튼 찾기 (danger variant)
    const deleteBtn = await $('button*=Delete All')
    if (await deleteBtn.isExisting()) {
      await deleteBtn.click()

      // ConfirmModal이 표시되는지 확인 (fixed inset-0 z-50 overlay)
      const modal = await $('.fixed.inset-0.z-50')
      await modal.waitForExist({ timeout: 3000 })
      expect(await modal.isDisplayed()).toBe(true)

      // Cancel 버튼으로 닫기
      const cancelBtn = await modal.$('button*=Cancel')
      if (await cancelBtn.isExisting()) {
        await cancelBtn.click()
      }
      // 또는 한국어: 취소
      else {
        const cancelKo = await modal.$('button*=취소')
        if (await cancelKo.isExisting()) await cancelKo.click()
      }
    }
  })
})
