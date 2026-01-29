/**
 * Cloudflare Worker: Cargo Sparse Registry
 *
 * Serves a sparse registry index from R2 storage.
 * Compatible with: cargo +nightly -Z sparse-registry
 *
 * Deploy: wrangler deploy
 */

export interface Env {
  CRATES_BUCKET: R2Bucket;
  INDEX_BUCKET: R2Bucket;
  AUTH_TOKEN?: string;
}

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const url = new URL(request.url);
    const path = url.pathname;

    // CORS headers
    const corsHeaders = {
      'Access-Control-Allow-Origin': '*',
      'Access-Control-Allow-Methods': 'GET, HEAD, OPTIONS',
      'Access-Control-Allow-Headers': 'Authorization',
    };

    if (request.method === 'OPTIONS') {
      return new Response(null, { headers: corsHeaders });
    }

    // Auth check for private registries
    if (env.AUTH_TOKEN) {
      const auth = request.headers.get('Authorization');
      if (auth !== `Bearer ${env.AUTH_TOKEN}`) {
        return new Response('Unauthorized', { status: 401 });
      }
    }

    try {
      // Registry config
      if (path === '/config.json') {
        return new Response(JSON.stringify({
          dl: `${url.origin}/api/v1/crates`,
          api: url.origin,
        }), {
          headers: {
            ...corsHeaders,
            'Content-Type': 'application/json',
            'Cache-Control': 'public, max-age=300',
          },
        });
      }

      // Sparse index: /index/{prefix}/{crate}
      // e.g., /index/ox/id/oxidizedgraph
      if (path.startsWith('/index/')) {
        const indexPath = path.replace('/index/', '');
        const object = await env.INDEX_BUCKET.get(indexPath);

        if (!object) {
          return new Response('Not Found', { status: 404, headers: corsHeaders });
        }

        return new Response(object.body, {
          headers: {
            ...corsHeaders,
            'Content-Type': 'text/plain',
            'Cache-Control': 'public, max-age=60',
            'ETag': object.etag,
          },
        });
      }

      // Download crate: /api/v1/crates/{name}/{version}/download
      if (path.startsWith('/api/v1/crates/') && path.endsWith('/download')) {
        const match = path.match(/\/api\/v1\/crates\/([^/]+)\/([^/]+)\/download/);
        if (!match) {
          return new Response('Bad Request', { status: 400, headers: corsHeaders });
        }

        const [, name, version] = match;
        const crateKey = `${name}/${name}-${version}.crate`;
        const object = await env.CRATES_BUCKET.get(crateKey);

        if (!object) {
          return new Response('Not Found', { status: 404, headers: corsHeaders });
        }

        return new Response(object.body, {
          headers: {
            ...corsHeaders,
            'Content-Type': 'application/octet-stream',
            'Content-Disposition': `attachment; filename="${name}-${version}.crate"`,
            'Cache-Control': 'public, max-age=31536000, immutable',
            'ETag': object.etag,
          },
        });
      }

      // Publish crate: PUT /api/v1/crates/new
      if (path === '/api/v1/crates/new' && request.method === 'PUT') {
        // Requires auth
        if (!env.AUTH_TOKEN) {
          return new Response('Publishing disabled', { status: 403, headers: corsHeaders });
        }

        const body = await request.arrayBuffer();
        const view = new DataView(body);

        // Parse cargo publish format:
        // - 4 bytes: JSON length (LE)
        // - JSON metadata
        // - 4 bytes: crate length (LE)
        // - crate tarball

        const jsonLen = view.getUint32(0, true);
        const jsonBytes = new Uint8Array(body, 4, jsonLen);
        const metadata = JSON.parse(new TextDecoder().decode(jsonBytes));

        const crateLen = view.getUint32(4 + jsonLen, true);
        const crateBytes = new Uint8Array(body, 8 + jsonLen, crateLen);

        const { name, vers: version } = metadata;

        // Store crate
        const crateKey = `${name}/${name}-${version}.crate`;
        await env.CRATES_BUCKET.put(crateKey, crateBytes);

        // Update index
        const indexPath = getIndexPath(name);
        const existingIndex = await env.INDEX_BUCKET.get(indexPath);
        const existingLines = existingIndex
          ? await existingIndex.text()
          : '';

        const indexLine = JSON.stringify({
          name,
          vers: version,
          deps: metadata.deps || [],
          cksum: await sha256(crateBytes),
          features: metadata.features || {},
          yanked: false,
        });

        const newIndex = existingLines
          ? `${existingLines}\n${indexLine}`
          : indexLine;

        await env.INDEX_BUCKET.put(indexPath, newIndex);

        return new Response(JSON.stringify({
          ok: true,
          warnings: { invalid_categories: [], invalid_badges: [], other: [] }
        }), {
          headers: { ...corsHeaders, 'Content-Type': 'application/json' },
        });
      }

      return new Response('Not Found', { status: 404, headers: corsHeaders });
    } catch (e) {
      console.error(e);
      return new Response('Internal Error', { status: 500, headers: corsHeaders });
    }
  },
};

// Get sparse index path for a crate name
function getIndexPath(name: string): string {
  const len = name.length;
  if (len === 1) return `1/${name}`;
  if (len === 2) return `2/${name}`;
  if (len === 3) return `3/${name[0]}/${name}`;
  return `${name.slice(0, 2)}/${name.slice(2, 4)}/${name}`;
}

// SHA256 checksum for crate
async function sha256(data: Uint8Array): Promise<string> {
  const hash = await crypto.subtle.digest('SHA-256', data);
  return Array.from(new Uint8Array(hash))
    .map(b => b.toString(16).padStart(2, '0'))
    .join('');
}
