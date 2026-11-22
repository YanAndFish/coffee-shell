-- CreateTable
CREATE TABLE "Product" (
    "id" TEXT NOT NULL PRIMARY KEY,
    "position" INTEGER NOT NULL,
    "code" TEXT NOT NULL,
    "name" TEXT NOT NULL,
    "subtitle" TEXT NOT NULL,
    "origin" TEXT NOT NULL,
    "region" TEXT NOT NULL,
    "farm" TEXT NOT NULL,
    "altitude" TEXT NOT NULL,
    "variety" TEXT NOT NULL,
    "process" TEXT NOT NULL,
    "harvest" TEXT NOT NULL,
    "roast" INTEGER NOT NULL,
    "tag" TEXT,
    "notes" TEXT NOT NULL,
    "flavor" TEXT NOT NULL,
    "score" REAL NOT NULL,
    "basePrice" INTEGER NOT NULL,
    "stock" INTEGER NOT NULL,
    "brew" TEXT NOT NULL,
    "description" TEXT NOT NULL,
    "soldOutWeights" TEXT NOT NULL DEFAULT '[]',
    "createdAt" DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "updatedAt" DATETIME NOT NULL
);

-- CreateIndex
CREATE UNIQUE INDEX "Product_position_key" ON "Product"("position");

-- CreateIndex
CREATE UNIQUE INDEX "Product_code_key" ON "Product"("code");
