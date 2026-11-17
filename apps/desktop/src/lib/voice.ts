import type { Id, WsServerMsg } from './types'

export interface VoiceMember { userId: Id; channelId: Id; muted: boolean; deafened: boolean; /** Server ms; missing from servers before 0.5.1. */ joinedAt?: number; /** When the call in the channel began (server ms); missing from older servers. */ callSince?: number }
export interface MyVoice { channelId: Id | null; muted: boolean; deafened: boolean }
export type VoiceEvent =
  | { kind: 'speaking'; userIds: Id[] }
  | { kind: 'level'; db: number; open: boolean }
  | { kind: 'path'; via: 'udp' | 'ws' }
  | { kind: 'deviceError'; message: string }
  | { kind: 'notice'; message: string }
  | { kind: 'state'; channelId: Id | null; muted: boolean; deafened: boolean }

export interface Prefs {
  inputDevice: string | null
  outputDevice: string | null
  vadThreshold: number
  volumes: Record<string, number>
  shortcutMute: string
  shortcutDeafen: string
  autostart: boolean
  streamPreset: string
  musicVolume: number
}

/** Who is in which voice channel, after one server message. Returns the same array when nothing changed. */
export function applyVoice(members: VoiceMember[], msg: WsServerMsg): VoiceMember[] {
  switch (msg.t) {
    case 'ready':
      return msg.d.voice ?? []
    case 'voiceMemberUpdated':
      return [...members.filter((m) => m.userId !== msg.d.userId), msg.d]
    case 'voiceMemberLeft':
      return members.filter((m) => !(m.userId === msg.d.userId && m.channelId === msg.d.channelId))
    case 'channelDeleted':
      return members.filter((m) => m.channelId !== msg.d.id)
    case 'userRemoved':
      return members.filter((m) => m.userId !== msg.d.id)
    default:
      return members
  }
}

export function membersOf(members: VoiceMember[], channelId: Id): VoiceMember[] {
  return members.filter((m) => m.channelId === channelId)
}
