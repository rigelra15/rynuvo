import type { KeyStatus } from './types'

/** The badge text beside a call member for their key status; null when nothing is shown. */
export function badgeOf(status: KeyStatus): string | null {
  switch (status) {
    case 'verified':
      return 'Verified'
    case 'changed':
      return 'Key changed — verify again'
    case 'newDevice':
      return 'New device'
    case 'known':
      return null
  }
}
