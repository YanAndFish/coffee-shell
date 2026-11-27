-- 同一账户的同一合并请求只保留一条记录。
CREATE TABLE "CartMerge" (
    "userId" INTEGER NOT NULL,
    "idempotencyKey" TEXT NOT NULL,
    "requestHash" TEXT NOT NULL,
    "mergedCount" INTEGER NOT NULL,
    "skippedCount" INTEGER NOT NULL,
    "createdAt" DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY ("userId", "idempotencyKey"),
    CONSTRAINT "CartMerge_userId_fkey" FOREIGN KEY ("userId") REFERENCES "User" ("id") ON DELETE CASCADE ON UPDATE CASCADE
);
