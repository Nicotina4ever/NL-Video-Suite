import { get, post } from './api';

interface Descriptor {
  mode: string;
  url?: string;
  job_id?: string;
  message?: string;
}
interface Job {
  status: string;
  message: string;
}

const cache = new Map<string, Promise<string>>();

// Resolve the browser-playable URL of an asset, triggering the same H.264
// proxy preparation the classic UI performs when the original codec is not
// universal. The promise is shared per project+asset.
export function playbackUrl(projectId: string, assetId: string): Promise<string> {
  const key = `${projectId}:${assetId}`;
  let pending = cache.get(key);
  if (!pending) {
    pending = resolve(projectId, assetId);
    cache.set(key, pending);
    pending.catch(() => cache.delete(key));
  }
  return pending;
}

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

async function resolve(projectId: string, assetId: string): Promise<string> {
  const descriptor = `/api/projects/${projectId}/assets/${assetId}/playback`;
  const first = await get<Descriptor>(descriptor);
  if ((first.mode === 'native' || first.mode === 'proxy') && first.url) return first.url;
  if (first.mode === 'proxy_required' || first.mode === 'preparing') {
    await post(descriptor, {}).catch(() => undefined);
  }
  for (let attempt = 0; attempt < 600; attempt++) {
    await sleep(500);
    const value = await get<Descriptor>(descriptor);
    if ((value.mode === 'native' || value.mode === 'proxy') && value.url) return value.url;
    if (value.mode === 'failed') throw new Error(value.message || 'Proxy di riproduzione non preparato');
    if (value.mode === 'preparing' && value.job_id) {
      const job = await get<Job>(`/api/jobs/${value.job_id}`).catch(() => null);
      if (job?.status === 'failed') throw new Error(job.message);
    }
  }
  throw new Error('Preparazione dell’anteprima ancora in corso');
}
