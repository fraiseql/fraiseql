// Upload a file to a FraiseQL Tus endpoint with the reference tus-js-client.
//
// FraiseQL implements the Tus 1.0.0 core exchange (creation, PATCH, HEAD,
// termination) by hand. Until this script existed, nothing had ever driven
// those endpoints with a real Tus client — every test spoke the protocol the
// same way the server did, so a shared misreading would have looked like
// agreement.
//
// Usage: node tus-upload.mjs <creation-url> <file> <chunk-size> [--expect-fail]
//
// Exits 0 on a completed upload, 1 on any error. With --expect-fail the
// expectation is inverted: the upload MUST fail, which is how the chunk-size
// refusals are proven to reach a real client as errors rather than as hangs.

import fs from 'node:fs'
import * as tus from 'tus-js-client'

const [creationUrl, filePath, chunkSizeArg, ...flags] = process.argv.slice(2)
if (!creationUrl || !filePath || !chunkSizeArg) {
  console.error('usage: node tus-upload.mjs <creation-url> <file> <chunk-size> [--expect-fail]')
  process.exit(2)
}
const expectFail = flags.includes('--expect-fail')
const data = fs.readFileSync(filePath)
const chunkSize = Number(chunkSizeArg)

const upload = new tus.Upload(data, {
  endpoint: creationUrl,
  chunkSize,
  uploadSize: data.length,
  // Tus clients advertise the content type through Upload-Metadata; FraiseQL
  // reads `filetype`/`contentType` from it and enforces the bucket's MIME
  // policy against what it finds.
  metadata: { filename: filePath.split('/').pop(), filetype: 'application/octet-stream' },
  retryDelays: null,
  onError(error) {
    if (expectFail) {
      console.log(`EXPECTED-FAIL ${error.message.replace(/\s+/g, ' ').slice(0, 400)}`)
      process.exit(0)
    }
    console.error(`tus upload failed: ${error.message}`)
    process.exit(1)
  },
  onSuccess() {
    if (expectFail) {
      console.error('tus upload succeeded but was expected to fail')
      process.exit(1)
    }
    console.log(`OK ${upload.url}`)
    process.exit(0)
  },
})

upload.start()
