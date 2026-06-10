#!/usr/bin/env node
import fs from 'fs'
import path from 'path'
import { fileURLToPath } from 'url'

const __dirname = path.dirname(fileURLToPath(import.meta.url))
const localesDir = path.join(__dirname, '../src/locales')

// Recursively flatten an object to dot-notation keys
function flattenKeys(obj, prefix = '') {
  const keys = new Set()
  for (const [k, v] of Object.entries(obj)) {
    // Skip _meta or special keys
    if (k === '_meta') continue
    const key = prefix ? `${prefix}.${k}` : k
    if (typeof v === 'object' && v !== null && !Array.isArray(v)) {
      for (const subkey of flattenKeys(v, key)) {
        keys.add(subkey)
      }
    } else {
      keys.add(key)
    }
  }
  return keys
}

// Read all .json files from locales dir
const files = fs.readdirSync(localesDir).filter(f => f.endsWith('.json'))
if (files.length === 0) {
  console.error('No locale files found in', localesDir)
  process.exit(1)
}

// Load en.json as reference
const enPath = path.join(localesDir, 'en.json')
if (!fs.existsSync(enPath)) {
  console.error('en.json not found. Locales must include en.json as reference.')
  process.exit(1)
}

const enData = JSON.parse(fs.readFileSync(enPath, 'utf-8'))
const enKeys = flattenKeys(enData)

let hasErrors = false

// Check each locale
for (const file of files) {
  if (file === 'en.json') continue // skip en.json itself

  const locPath = path.join(localesDir, file)
  const locData = JSON.parse(fs.readFileSync(locPath, 'utf-8'))
  const locKeys = flattenKeys(locData)

  const locale = file.replace('.json', '')
  const missing = new Set([...enKeys].filter(k => !locKeys.has(k)))
  const extra = new Set([...locKeys].filter(k => !enKeys.has(k)))

  if (missing.size > 0 || extra.size > 0) {
    hasErrors = true
    console.error(`\nERROR: ${locale}.json out of sync with en.json`)
    if (missing.size > 0) {
      console.error(`  Missing keys (${missing.size}):`)
      for (const k of Array.from(missing).sort()) {
        console.error(`    - ${k}`)
      }
    }
    if (extra.size > 0) {
      console.error(`  Extra keys (${extra.size}):`)
      for (const k of Array.from(extra).sort()) {
        console.error(`    - ${k}`)
      }
    }
  } else {
    console.log(`✓ ${locale}.json in parity`)
  }
}

if (hasErrors) {
  process.exit(1)
}

console.log('\n✓ All locales are in sync')
process.exit(0)
