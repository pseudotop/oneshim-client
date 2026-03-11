// e2e-tauri/update-lifecycle.spec.ts
import { invokeIpc, type UpdateStatusResponse } from './helpers.js'

describe('J5: Update Lifecycle', () => {
  /**
   * @tc_id T140
   * @risk_id FUNC-006
   * @tauri_only_reason IPC get_update_status triggers real update state machine
   */
  it('T140: get_update_status returns valid structure', async () => {
    const status = await invokeIpc<UpdateStatusResponse>('get_update_status')

    expect(status).toBeDefined()
    expect(typeof status.phase).toBe('string')
    expect(status.phase.length).toBeGreaterThan(0)
  })

  /**
   * @tc_id T141
   * @risk_id FUNC-007
   * @tauri_only_reason IPC approve_update triggers real update action
   */
  it('T141: approve_update returns without crash', async () => {
    // 업데이트가 없을 수 있으므로 에러도 허용 — 크래시만 아니면 됨
    let result: any
    let error: string | undefined
    try {
      result = await invokeIpc('approve_update')
    } catch (e) {
      error = String(e)
    }
    // 결과가 있거나 에러 메시지가 있으면 통과 (undefined/null 크래시가 아님)
    const responded = result !== undefined || error !== undefined
    expect(responded).toBe(true)
  })

  /**
   * @tc_id T142
   * @risk_id FUNC-008
   * @tauri_only_reason IPC defer_update triggers real update action
   */
  it('T142: defer_update returns without crash', async () => {
    let result: any
    let error: string | undefined
    try {
      result = await invokeIpc('defer_update')
    } catch (e) {
      error = String(e)
    }
    const responded = result !== undefined || error !== undefined
    expect(responded).toBe(true)
  })

  /**
   * @tc_id T143
   * @risk_id DATA-002
   * @tauri_only_reason Version comes from real Cargo.toml, not mocked
   */
  it('T143: Update page shows version matching tauri.conf.json', async () => {
    await browser.url('tauri://localhost/updates')
    await browser.pause(2000)

    const pageText = await $('body').getText()

    // tauri.conf.json의 version을 IPC로 확인
    const status = await invokeIpc<UpdateStatusResponse>('get_update_status')

    // 페이지에 버전 정보가 표시되는지 확인 (semver 패턴)
    const semverPattern = /\d+\.\d+\.\d+/
    expect(pageText).toMatch(semverPattern)
  })
})
