import type { ErrorCode } from './common.ts'
import type { Endpoint } from './endpoint.ts'
import { z } from 'zod'
import { components, endpoints } from './api.ts'
import { AUTHORIZATION_HEADER, CLIENT_IP_HEADER, ERRORS, INTERNAL_KEY_HEADER, SSH_FINGERPRINT_HEADER } from './common.ts'
import { endpointErrorCodes } from './endpoint.ts'

const COMPONENT_REF = '#/components/schemas/'

type JsonObject = Record<string, unknown>

/** Drops the keys a standalone `z.toJSONSchema` adds that do not belong inside OpenAPI. */
function stripSchema(schema: JsonObject): JsonObject {
  const { $schema: _schema, $id: _id, ...rest } = schema
  return rest
}

/**
 * Converts the named zod schemas into OpenAPI 3.1 (JSON Schema 2020-12) components. Input
 * mode: request fields with defaults are optional (response schemas have no defaults, so
 * both modes agree on them).
 */
function componentSchemas(): Record<string, JsonObject> {
  const registry = z.registry<{ id: string }>()
  for (const [id, schema] of Object.entries(components))
    registry.add(schema, { id })
  const { schemas } = z.toJSONSchema(registry, {
    uri: id => `${COMPONENT_REF}${id}`,
    io: 'input',
  })
  const out: Record<string, JsonObject> = {}
  for (const id of Object.keys(components))
    out[id] = stripSchema(schemas[id] as JsonObject)
  return out
}

/** Finds the component name of a schema so responses and bodies can `$ref` it. */
function refOf(schema: z.ZodType): JsonObject {
  const entry = Object.entries(components).find(([, s]) => s === schema)
  if (!entry)
    throw new Error('Request and response schemas must be registered in components (src/api.ts).')
  return { $ref: `${COMPONENT_REF}${entry[0]}` }
}

function jsonContent(schema: z.ZodType): JsonObject {
  return {
    'application/json': {
      schema: refOf(schema),
    },
  }
}

/** OpenAPI parameters of a path or query object schema. */
function parameters(where: 'path' | 'query', schema: z.ZodObject | undefined): JsonObject[] {
  if (!schema)
    return []
  return Object.entries(schema.shape).map(([name, property]) => {
    const json = stripSchema(z.toJSONSchema(property as z.ZodType, { io: 'input' }) as JsonObject)
    const { description, ...rest } = json
    return {
      name,
      in: where,
      required: where === 'path' || !(property as z.ZodType).safeParse(undefined).success,
      ...(typeof description === 'string'
        ? { description }
        : {}),
      schema: rest,
    }
  })
}

function headerParameters(endpoint: Endpoint): JsonObject[] {
  const out: JsonObject[] = []
  if (endpoint.fingerprint !== 'none') {
    out.push({
      name: SSH_FINGERPRINT_HEADER,
      in: 'header',
      required: endpoint.fingerprint === 'required',
      description: '本次 SSH 连接的公钥指纹（`SHA256:…`），由 TUI 从会话环境转发；只在带内部密钥的请求里可信。',
      schema: { type: 'string', pattern: '^SHA256:[A-Za-z0-9+/]{43}$' },
    })
  }
  if (endpoint.tag === 'auth' && endpoint.auth === 'none') {
    out.push({
      name: CLIENT_IP_HEADER,
      in: 'header',
      required: false,
      description: 'SSH 客户端的 IP（`COFFEE_CLIENT_IP`），仅用于限流与日志。',
      schema: { type: 'string', maxLength: 64 },
    })
  }
  return out
}

/** `x-error-codes` lists, and one response per status naming its codes and messages. */
function errorResponses(endpoint: Endpoint): JsonObject {
  const errorEnvelope = jsonContent(components.ApiError)
  const byStatus = new Map<number, ErrorCode[]>()
  for (const code of endpointErrorCodes(endpoint)) {
    const { status } = ERRORS[code]
    byStatus.set(status, [
      ...(byStatus.get(status) ?? []),
      code,
    ])
  }
  const responses: JsonObject = {}
  for (const [status, codes] of [...byStatus].sort(([a], [b]) => a - b)) {
    responses[String(status)] = {
      description: codes.map(code => `\`${code}\` ${ERRORS[code].message}`).join('；'),
      content: errorEnvelope,
    }
  }
  return responses
}

function security(endpoint: Endpoint): JsonObject[] | undefined {
  const internal = endpoint.access === 'internal'
  switch (endpoint.auth) {
    case 'user':
      return [
        internal
          ? { internalKey: [], bearer: [] }
          : { bearer: [] },
      ]
    case 'optional':
      return [
        internal
          ? { internalKey: [], bearer: [] }
          : { bearer: [] },
        internal
          ? { internalKey: [] }
          : {},
      ]
    case 'none':
      return internal
        ? undefined
        : []
  }
}

function operation(endpoint: Endpoint): JsonObject {
  const responses: JsonObject = {}
  const { status, schema } = endpoint.success
  responses[String(status)] = schema
    ? {
        description: '成功',
        content: jsonContent(schema),
      }
    : { description: '成功' }
  Object.assign(responses, errorResponses(endpoint))
  responses.default = {
    description: '错误（统一错误信封）',
    content: jsonContent(components.ApiError),
  }
  const params = [
    ...parameters('path', endpoint.request?.params),
    ...parameters('query', endpoint.request?.query),
    ...headerParameters(endpoint),
  ]
  const sec = security(endpoint)
  return {
    'operationId': endpoint.operationId,
    'tags': [endpoint.tag],
    'summary': endpoint.summary,
    ...(endpoint.description
      ? { description: endpoint.description }
      : {}),
    ...(sec
      ? { security: sec }
      : {}),
    ...(params.length > 0
      ? { parameters: params }
      : {}),
    ...(endpoint.request?.body
      ? {
          requestBody: {
            required: true,
            content: jsonContent(endpoint.request.body),
          },
        }
      : {}),
    responses,
    'x-auth': endpoint.auth,
    'x-error-codes': endpointErrorCodes(endpoint),
  }
}

/** `{ code: { status, message } }` for every error code, documented once at the top. */
function errorTable(): string {
  return Object.entries(ERRORS)
    .map(([code, { status, message }]) => `| \`${code}\` | ${status} | ${message} |`)
    .join('\n')
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
      version: '0.2.0',
      description: [
        'ROAST/TTY 终端咖啡馆的后端接口。这是演示站，不是真的商店：不接支付，下单即完成。',
        '',
        '- 金额一律是整数元，全部由服务端计算；TUI 只负责显示。',
        `- 除 \`/v1/health\` 外，后端设置了 COFFEE_INTERNAL_KEY 时每个请求都要带 \`${INTERNAL_KEY_HEADER}\`。`,
        `- 会员会话：\`${AUTHORIZATION_HEADER}: Bearer <token>\`（注册、登录、恢复登录时得到）。`,
        `- \`${SSH_FINGERPRINT_HEADER}\`：本次 SSH 连接的公钥指纹，用于“记住我”；\`${CLIENT_IP_HEADER}\`：客户端 IP，仅用于限流。`,
        '- 日期：`*At` 是 UTC 时刻（ISO 8601）；`*Date` / `*On` 是店铺时区（Asia/Shanghai）的日历日期。',
        '- 错误一律是 `{ "error": { "code", "message", "details"? } }`，`message` 与 code 一一对应：',
        '',
        '| code | HTTP | message |',
        '| --- | --- | --- |',
        errorTable(),
      ].join('\n'),
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
          description: '网关与后端之间的共享密钥；仅当后端设置了 COFFEE_INTERNAL_KEY 时校验。',
        },
        bearer: {
          type: 'http',
          scheme: 'bearer',
          description: '会员会话令牌（43 个 base64url 字符，7 天有效）。',
        },
      },
      schemas: componentSchemas(),
    },
  }
}
