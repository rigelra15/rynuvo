import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { Channel, ChannelKind, Id, Message, User, WsServerMsg } from './types'

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
  join: (server: string, invite: string, nickname: string) => invoke<User>('join', { server, invite, nickname }),
  claimTransfer: (server: string, code: string) => invoke<User>('claim_transfer', { server, code }),
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
}

export interface Report { id: Id; userId: Id; nickname: string; kind: string; body: string; createdAt: number }

export function onSessionEvent(cb: (e: SessionEvent) => void): Promise<UnlistenFn> {
  return listen<SessionEvent>('rynuvo://event', (e) => cb(e.payload))
}

/** Tauri command errors arrive as plain strings. */
export function errText(e: unknown): string {
  return typeof e === 'string' ? e : e instanceof Error ? e.message : 'Terjadi kesalahan'
}
