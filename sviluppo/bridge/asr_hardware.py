import json
result = {'cpu': True, 'cuda': False, 'reason': '', 'qwen_gguf_execution': 'cpu', 'downloads': False}
try:
    import torch
    import ctranslate2
    result.update(torch=torch.__version__, cuda_build=torch.version.cuda, ctranslate2=ctranslate2.__version__, torch_cuda=torch.cuda.is_available(), cuda_devices=ctranslate2.get_cuda_device_count())
    result['cuda'] = bool(result['torch_cuda'] and result['cuda_devices'] > 0)
    result['reason'] = '' if result['cuda'] else 'Torch e CTranslate2 devono entrambi avere CUDA disponibile.'
    if result['cuda']:
        result['gpu_name'] = torch.cuda.get_device_name(0)
except Exception as error:
    result['reason'] = str(error)
print(json.dumps(result))
