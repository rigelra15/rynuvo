import { describe, expect, it } from 'vitest'
import { describeNet, type NetReport } from './api'

const base: NetReport = { ipv4Mapped: [null, null], ipv6Mapped: [null, null], localIps: [], mapping: 'unknown', udpOk: false, ipv6Ok: false, uploadMbps: null }

describe('describeNet', () => {
  it('says direct works for endpoint-independent mapping', () => {
    const r = describeNet({ ...base, udpOk: true, mapping: 'endpointIndependent', uploadMbps: 12.5 })
    expect(r[0]).toContain('very likely works')
    expect(r[2]).toBe('Upload to server: 12.5 Mbps.')
  })
  it('falls back to server when UDP is blocked', () => {
    expect(describeNet(base)[0]).toContain('UDP looks blocked')
  })
  it('treats IPv6 as direct-capable even behind symmetric NAT', () => {
    expect(describeNet({ ...base, udpOk: true, ipv6Ok: true, mapping: 'dependent' })[0]).toContain('very likely works')
  })
})
