import type { ApiError } from '../common.ts'

/** `fixtures/v1/error.json`: the 404 envelope. */
export const notFoundExample: ApiError = {
  error: {
    code: 'not_found',
    message: '资源不存在',
  },
}
