import { describe, expect, it } from 'vitest'
import {
  applyCapture,
  createEnvironment,
  nextEnvironmentColor,
  requestEnvironment,
  rootGroup,
  type Environment,
} from '../environments'
import { resolveTokenDefinitions } from '../authorization'
import { createSession } from '../session'
import { decodeWorkspace, encodeWorkspace } from '../workspace'
import type { RequestGroup } from '../groups'

function tree() {
  const dev: Environment = {
    ...createEnvironment('DEV', 'info'),
    values: { host: 'dev' },
  }
  const prod: Environment = {
    ...createEnvironment('PROD', 'destructive'),
    protected: true,
    values: { host: 'prod', key: 'p' },
  }
  const groups: RequestGroup[] = [
    {
      id: 1,
      name: 'API',
      parentId: null,
      collapsed: false,
      localDefinitions: { host: 'base', version: 'v1' },
      environments: [dev, prod],
      activeEnvironmentId: dev.id,
    },
    {
      id: 2,
      name: 'Users',
      parentId: 1,
      collapsed: false,
      localDefinitions: { version: 'v2' },
    },
  ]
  return { groups, dev, prod }
}

describe('environments', () => {
  it('finds the root group and its active environment', () => {
    const { groups, dev } = tree()
    expect(rootGroup(2, groups)?.id).toBe(1)
    expect(rootGroup(null, groups)).toBeUndefined()
    expect(requestEnvironment(2, groups)?.id).toBe(dev.id)
  })
  it('puts active values over base tokens; nested groups still win', () => {
    const { groups, prod } = tree()
    expect(resolveTokenDefinitions(2, groups, {}).definitions).toEqual({
      host: 'dev',
      version: 'v2',
    })
    groups[0].activeEnvironmentId = prod.id
    expect(resolveTokenDefinitions(1, groups, {}).definitions).toEqual({
      host: 'prod',
      version: 'v1',
      key: 'p',
    })
    groups[0].activeEnvironmentId = null
    expect(resolveTokenDefinitions(1, groups, {}).definitions.host).toBe('base')
  })
  it('ignores environments on a group that is no longer a root', () => {
    const { groups } = tree()
    groups.unshift({ id: 3, name: 'Top', parentId: null, collapsed: false })
    groups[1].parentId = 3
    expect(resolveTokenDefinitions(1, groups, {}).definitions.host).toBe('base')
  })
  it('captures into the active environment, else the root, else globals', () => {
    const { groups, dev, prod } = tree()
    expect(applyCapture(2, groups, { token: 'a' })).toBe('environment')
    expect(dev.values.token).toBe('a')
    expect(prod.values.token).toBeUndefined()
    groups[0].activeEnvironmentId = null
    expect(applyCapture(2, groups, { token: 'b' })).toBe('group')
    expect(groups[0].localDefinitions?.token).toBe('b')
    expect(applyCapture(null, groups, { token: 'c' })).toBe('global')
  })
  it('picks an unused color', () => {
    const { dev, prod } = tree()
    expect(nextEnvironmentColor([dev, prod])).toBe('keyword')
  })
  it('round-trips and validates environments', () => {
    const { groups } = tree()
    const session = createSession()
    const encoded = encodeWorkspace([session], session.id, groups)
    expect(decodeWorkspace(encoded).groups).toEqual(groups)
    const bad = JSON.parse(encoded)
    bad.groups[0].activeEnvironmentId = 999
    expect(() => decodeWorkspace(JSON.stringify(bad))).toThrow()
    bad.groups[0].activeEnvironmentId = null
    bad.groups[0].environments[0].color = '#ff0000'
    expect(() => decodeWorkspace(JSON.stringify(bad))).toThrow()
  })
})
