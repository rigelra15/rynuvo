import { describe, expect, test, vi } from 'vitest'
import { catchUp, videoUrls } from './video'

describe('video urls', () => {
  test('each video is asked for once, and asked again after a failure', async () => {
    const get = vi.fn().mockRejectedValueOnce(new Error('offline')).mockResolvedValue('https://v/1')
    const urls = videoUrls(get)
    await expect(urls.get('a')).rejects.toThrow()
    expect(await urls.get('a')).toBe('https://v/1')
    expect(await urls.get('a')).toBe('https://v/1')
    expect(get).toHaveBeenCalledTimes(2)
  })
  test('prefetch asks for the playing video and the next one, not songs', () => {
    const get = vi.fn().mockResolvedValue('u')
    const urls = videoUrls(get)
    const e = (videoId: string, video: boolean) => ({ videoId, video })
    urls.prefetch({ now: e('now', true), queue: [e('next', true), e('later', true)] })
    urls.prefetch({ now: e('song', false), queue: [] })
    expect(get.mock.calls.map((c) => c[0])).toEqual(['now', 'next'])
  })
})

describe('keeping a silent video in step with the song', () => {
  test('far off: jump to the right spot', () => {
    expect(catchUp(1.2)).toEqual({ seek: true, rate: 1 })
    expect(catchUp(-0.9)).toEqual({ seek: true, rate: 1 })
  })
  test('a little behind or ahead: run a touch faster or slower', () => {
    expect(catchUp(-0.2)).toEqual({ seek: false, rate: 1.05 })
    expect(catchUp(0.2)).toEqual({ seek: false, rate: 0.95 })
  })
  test('close enough: normal speed', () => {
    expect(catchUp(0.03)).toEqual({ seek: false, rate: 1 })
  })
})
