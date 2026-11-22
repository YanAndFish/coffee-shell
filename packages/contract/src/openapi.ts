import type { Endpoint } from './endpoint.ts'
import { z } from 'zod'
import { components, endpoints } from './api.ts'
import { INTERNAL_KEY_HEADER } from './common.ts'

const COMPONENT_REF = '#/components/schemas/'

/** Chinese descriptions of the error statuses endpoints may document. */
const ERROR_DESCRIPTIONS: Record<number, string> = {
  400: '请求格式错误或参数不合法',
  401: '缺少或错误的内部密钥',
  404: '资源不存在',
  409: '状态冲突',
  422: '请求参数不合法',
}

type JsonObject = Record<string, unknown>

/** Converts the named zod schemas into OpenAPI 3.1 (JSON Schema 2020-12) components. */
function componentSchemas(): Record<string, JsonObject> {
  const registry = z.registry<{ id: string }>()
  for (const [id, schema] of Object.entries(components))
    registry.add(schema, { id })
  const { schemas } = z.toJSONSchema(registry, {
    uri: id => `${COMPONENT_REF}${id}`,
  })
  const out: Record<string, JsonObject> = {}
  for (const id of Object.keys(components)) {
    const { $schema: _schema, $id: _id, ...rest } = schemas[id] as JsonObject
    out[id] = rest
  }
  return out
}

/** Finds the component name of a schema so responses can `$ref` it. */
function refOf(schema: z.ZodType): JsonObject {
  const entry = Object.entries(components).find(([, s]) => s === schema)
  if (!entry)
    throw new Error('Response schemas must be registered in components (src/api.ts).')
  return { $ref: `${COMPONENT_REF}${entry[0]}` }
}

function jsonContent(schema: z.ZodType): JsonObject {
  return {
    'application/json': {
      schema: refOf(schema),
    },
  }
}

function operation(endpoint: Endpoint): JsonObject {
  const errorEnvelope = jsonContent(components.ApiError)
  const responses: JsonObject = {}
  const { status, schema } = endpoint.success
  responses[String(status)] = schema
    ? {
        description: '成功',
        content: jsonContent(schema),
      }
    : { description: '成功' }
  for (const code of endpoint.errors)
    responses[String(code)] = { description: ERROR_DESCRIPTIONS[code] ?? '错误', content: errorEnvelope }
  responses.default = { description: '错误（统一错误信封）', content: errorEnvelope }
  return {
    operationId: endpoint.operationId,
    tags: [endpoint.tag],
    summary: endpoint.summary,
    ...(endpoint.access === 'public'
      ? { security: [] }
      : {}),
    responses,
  }
}

/** Builds the OpenAPI document of the whole API. */
export function buildOpenApi(): JsonObject {
  const paths: Record<string, JsonObject> = {}
  for (const endpoint of endpoints) {
    const item = (paths[endpoint.path] ??= {})
    if (item[endpoint.method])
      throw new Error(`Duplicate endpoint ${endpoint.method.toUpperCase()} ${endpoint.path}`)
    item[endpoint.method] = operation(endpoint)
  }
  return {
    openapi: '3.1.0',
    info: {
      title: 'ROAST/TTY API',
      version: '0.1.0',
      description: 'ROAST/TTY 终端咖啡馆的后端接口。这是演示站，不是真的商店。金额单位为整数元。',
    },
    servers: [
      { url: 'http://127.0.0.1:3000' },
    ],
    security: [
      { internalKey: [] },
    ],
    paths,
    components: {
      securitySchemes: {
        internalKey: {
          type: 'apiKey',
          in: 'header',
          name: INTERNAL_KEY_HEADER,
          description: '仅当后端设置了 COFFEE_INTERNAL_KEY 时校验。',
        },
      },
      schemas: componentSchemas(),
    },
  }
}
