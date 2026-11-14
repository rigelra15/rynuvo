// Mirrors crates/rynuvo-proto (serde camelCase).
import type { VoiceMember } from './voice'
import type { StreamInfo } from './stream'
export type Id = number
export type ChannelKind = 'text' | 'voice'
export interface User { id: Id; nickname: string; isAdmin: boolean; avatarIcon?: string | null; avatarColor?: string | null }
export interface Channel { id: Id; kind: ChannelKind; name: string; topic: string | null; ownerId: Id; position: number }
export interface ImageRef { id: string; width: number; height: number; mime: string }
export interface Message { id: Id; channelId: Id; authorId: Id; content: string; image: ImageRef | null; createdAt: number; editedAt: number | null }

export type WsServerMsg =
  | { t: 'ready'; d: { me: User; users: User[]; channels: Channel[]; online: Id[]; alerts: string[]; voice?: VoiceMember[]; streams?: StreamInfo[] } }
  | { t: 'messageCreated'; d: Message }
  | { t: 'messageUpdated'; d: Message }
  | { t: 'messageDeleted'; d: { id: Id; channelId: Id } }
  | { t: 'channelCreated'; d: Channel }
  | { t: 'channelUpdated'; d: Channel }
  | { t: 'channelDeleted'; d: { id: Id } }
  | { t: 'userUpserted'; d: User }
  | { t: 'userRemoved'; d: { id: Id } }
  | { t: 'presence'; d: { userId: Id; online: boolean } }
  | { t: 'typing'; d: { channelId: Id; userId: Id } }
  | { t: 'adminAlert'; d: { message: string } }
  | { t: 'voiceJoined'; d: { channelId: Id; session: number; key: string; udpPort: number } }
  | { t: 'voiceMemberUpdated'; d: VoiceMember }
  | { t: 'voiceMemberLeft'; d: { userId: Id; channelId: Id } }
  | { t: 'voiceEnded'; d: { reason: string } }
  | { t: 'streamStarted'; d: StreamInfo }
  | { t: 'streamStopped'; d: { userId: Id; channelId: Id } }
  | { t: 'streamViewer'; d: { streamer: Id; viewer: Id; codecs: string[]; key: string; joined: boolean } }
  | { t: 'streamSignal'; d: { from: Id; data: unknown } }
  | { t: 'pong' }
