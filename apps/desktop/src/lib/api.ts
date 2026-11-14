import { invoke, convertFileSrc } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { Channel, ChannelKind, Id, ImageRef, Message, User, WsServerMsg } from './types'

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
  uploadImage: (bytes: Uint8Array) => invoke<ImageRef>('upload_image', bytes),
  postImageMessage: (channel: Id, content: string, imageId: string) =>
    invoke<Message>('post_image_message', { channel, content, imageId }),
  saveImage: (id: string) => invoke<string>('save_image', { id }),
  runNettest: () => invoke<NetReport>('run_nettest'),
  sendLog: () => invoke<void>('send_log'),
  appVersion: () => invoke<string>('app_version'),
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

export interface Report { id: Id; userId: Id; nickname: string; kind: string; body: string; createdAt: number }

export function onSessionEvent(cb: (e: SessionEvent) => void): Promise<UnlistenFn> {
  return listen<SessionEvent>('rynuvo://event', (e) => cb(e.payload))
}

/** Tauri command errors arrive as plain strings. */
export function errText(e: unknown): string {
  return typeof e === 'string' ? e : e instanceof Error ? e.message : 'Something went wrong'
}

export const mediaUrl = (id: string) => convertFileSrc(id, 'rynuvo-media')
