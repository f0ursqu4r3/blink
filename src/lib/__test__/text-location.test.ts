import { describe, expect, it } from 'vitest'
import { formatGraphql, graphqlErrorLocation } from '../graphql'
import { formatJson, jsonErrorLocation } from '../json'
import { describeLocation, locationFromOffset } from '../text-location'

async function graphqlError(text: string) {
  try {
    await formatGraphql(text)
  } catch (error) {
    return error
  }
  throw new Error('expected formatGraphql to throw')
}
function jsonError(text: string) {
  try {
    formatJson(text)
  } catch (error) {
    return error
  }
  throw new Error('expected formatJson to throw')
}

describe('locationFromOffset', () => {
  it('converts an offset to a 1-based line and column', () => {
    expect(locationFromOffset('ab\ncd\nef', 4, 'x')).toEqual({
      line: 2,
      column: 2,
      offset: 4,
      reason: 'x',
    })
  })
  it('clamps offsets past the end of the text', () => {
    expect(locationFromOffset('ab', 99, 'x')).toMatchObject({
      line: 1,
      column: 3,
      offset: 2,
    })
  })
  it('describes a location', () => {
    expect(describeLocation(locationFromOffset('a\nb', 2, 'x'))).toBe('line 2, column 1')
  })
})

describe('jsonErrorLocation', () => {
  it('locates an error in the middle of the text', () => {
    const text = '{\n  "a": 1,\n  "b": }'
    expect(jsonErrorLocation(text, jsonError(text))).toEqual({
      line: 3,
      column: 8,
      offset: 19,
      reason: "Object value expected after ':'",
    })
  })
  it('locates an error at the end of the text', () => {
    const text = '{"a":1'
    expect(jsonErrorLocation(text, jsonError(text))).toMatchObject({
      line: 1,
      column: 7,
      offset: 6,
    })
  })
  it('returns null when the error has no position', () => {
    expect(jsonErrorLocation('{}', new Error('boom'))).toBeNull()
    expect(jsonErrorLocation('{}', 'boom')).toBeNull()
  })
})

describe('graphqlErrorLocation', () => {
  it('locates an error in the middle of the text', async () => {
    const text = 'query {\n  viewer(id: ) {\n    id\n  }\n}'
    const location = graphqlErrorLocation(text, await graphqlError(text))
    expect(location).toMatchObject({ line: 2, column: 14, offset: 21 })
    expect(location?.reason).not.toMatch(/Syntax Error|\(\d+:\d+\)/)
  })
  it('locates an error at the end of the text', async () => {
    const text = 'query {\n  viewer {\n    id\n  \n'
    expect(graphqlErrorLocation(text, await graphqlError(text))).toMatchObject({
      line: 5,
      column: 1,
      offset: text.length,
    })
  })
  it('returns null when the error has no location', () => {
    expect(graphqlErrorLocation('{a}', new Error('boom'))).toBeNull()
  })
})
