import { invoke, convertFileSrc } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { Channel, ChannelKind, Id, ImageRef, Message, User, WsServerMsg } from './types'
import type { MyVoice, Prefs, VoiceEvent } from './voice'

export type ConnState = { state: 'connecting' } | { state: 'online' } | { state: 'reconnecting'; inMs: number }

export type SessionEvent =
  | { kind: 'server'; data: WsServerMsg }
  | { kind: 'conn'; data: ConnState }
  | { kind: 'revoked' }
  | { kind: 'updateRequired' }

export interface BootInfo { server: string; me: User | null }
export interface TransferCode { code: string; expiresAt: number }

export const api = {
  bootstrap: () => invoke<BootInfo | null>('bootstrap'),
  join: (invite: string, nickname: string) => invoke<User>('join', { invite, nickname }),
  claimTransfer: (code: string) => invoke<User>('claim_transfer', { code }),
  logout: () => invoke<void>('logout'),
  renameMe: (nickname: string) => invoke<User>('rename_me', { nickname }),
  createChannel: (kind: ChannelKind, name: string) => invoke<Channel>('create_channel', { kind, name }),
  updateChannel: (id: Id, req: { name?: string; topic?: string | null; position?: number }) =>
    invoke<Channel>('update_channel', { id, req }),
  deleteChannel: (id: Id) => invoke<void>('delete_channel', { id }),
  messages: (channel: Id, before?: Id) => invoke<Message[]>('messages', { channel, before: before ?? null }),
  postMessage: (channel: Id, content: string) => invoke<Message>('post_message', { channel, content }),
  editMessage: (id: Id, content: string) => invoke<Message>('edit_message', { id, content }),
  deleteMessage: (id: Id) => invoke<void>('delete_message', { id }),
  sendTyping: (channel: Id) => invoke<void>('send_typing', { channel }),
  transferCode: () => invoke<TransferCode>('transfer_code'),
  adminRotateInvite: () => invoke<{ code: string }>('admin_rotate_invite'),
  adminKick: (user: Id) => invoke<void>('admin_kick', { user }),
  adminTransferCode: (user: Id) => invoke<TransferCode>('admin_transfer_code', { user }),
  adminReports: () => invoke<Report[]>('admin_reports'),
  adminReportBody: (id: Id) => invoke<string>('admin_report_body', { id }),
  uploadImage: (bytes: Uint8Array) => invoke<ImageRef>('upload_image', bytes),
  postImageMessage: (channel: Id, content: string, imageId: string) =>
    invoke<Message>('post_image_message', { channel, content, imageId }),
  saveImage: (id: string) => invoke<string>('save_image', { id }),
  runNettest: () => invoke<NetReport>('run_nettest'),
  sendLog: () => invoke<void>('send_log'),
  appVersion: () => invoke<string>('app_version'),
  voiceState: () => invoke<MyVoice>('voice_state'),
  voiceJoin: (channelId: Id) => invoke<MyVoice>('voice_join', { channelId }),
  voiceLeave: () => invoke<MyVoice>('voice_leave'),
  voiceSetMuted: (muted: boolean) => invoke<MyVoice>('voice_set_muted', { muted }),
  voiceSetDeafened: (deafened: boolean) => invoke<MyVoice>('voice_set_deafened', { deafened }),
  voiceSetVolume: (userId: Id, volume: number) => invoke<void>('voice_set_volume', { userId, volume }),
  voiceDevices: () => invoke<{ inputs: string[]; outputs: string[] }>('voice_devices'),
  voiceSetDevices: (input: string | null, output: string | null) => invoke<void>('voice_set_devices', { input, output }),
  voiceSetThreshold: (threshold: number) => invoke<void>('voice_set_threshold', { threshold }),
  voiceMicTest: (on: boolean) => invoke<void>('voice_mic_test', { on }),
  voiceKick: (userId: Id) => invoke<void>('voice_kick', { userId }),
  prefsGet: () => invoke<Prefs>('prefs_get'),
  prefsSet: (next: Prefs) => invoke<Prefs>('prefs_set', { next }),
}

export function onVoiceEvent(cb: (e: VoiceEvent) => void): Promise<UnlistenFn> {
  return listen<VoiceEvent>('rynuvo://voice', (e) => cb(e.payload))
}

export type Mapping = 'endpointIndependent' | 'dependent' | 'unknown'
export interface NetReport {
  ipv4Mapped: (string | null)[]
  ipv6Mapped: (string | null)[]
  localIps: string[]
  mapping: Mapping
  udpOk: boolean
  ipv6Ok: boolean
  uploadMbps: number | null
}

/** Plain-language summary of a connection test. */
export function describeNet(r: NetReport): string[] {
  const p2p = !r.udpOk
    ? 'UDP looks blocked: calls and screen share will go through the server.'
    : r.mapping === 'endpointIndependent' || r.ipv6Ok
      ? 'Direct connection with friends: very likely works.'
      : 'Direct connection with friends: may not work, so it might go through the server.'
  return [
    p2p,
    r.ipv6Ok ? 'IPv6: available.' : 'IPv6: not available.',
    r.uploadMbps != null ? `Upload to server: ${r.uploadMbps} Mbps.` : 'Upload test failed.',
  ]
}

/** `body` is a preview; `size` is the full length. */
export interface Report { id: Id; userId: Id; nickname: string; kind: string; body: string; size: number; createdAt: number }

export function onSessionEvent(cb: (e: SessionEvent) => void): Promise<UnlistenFn> {
  return listen<SessionEvent>('rynuvo://event', (e) => cb(e.payload))
}

/** Tauri command errors arrive as plain strings. */
export function errText(e: unknown): string {
  return typeof e === 'string' ? e : e instanceof Error ? e.message : 'Something went wrong'
}

export const mediaUrl = (id: string) => convertFileSrc(id, 'rynuvo-media')
