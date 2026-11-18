"""Opt-in REAL local inference. Separate synthetic project; no personal media.

CPU Ollama story, live-schema LTX repair on a copy, Flux2 image generation.
Retains output for human review; does not approve or publish the generated media.
No model downloads, service starts, global unloads, or legacy-folder writes.
"""
import ctypes
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time
import uuid
sys.path.insert(0, str(Path(__file__).resolve().parent))
from live import request
from smoke import free_port
from objects_http import png_bytes

ROOT = Path(__file__).resolve().parents[2]


def available_ram():
    class Memory(ctypes.Structure):
        _fields_ = [("length", ctypes.c_ulong), ("load", ctypes.c_ulong)] + [(k, ctypes.c_ulonglong) for k in (
            "total", "available", "total_page", "available_page", "total_virtual", "available_virtual", "extended")]
    value = Memory(); value.length = ctypes.sizeof(value)
    assert ctypes.windll.kernel32.GlobalMemoryStatusEx(ctypes.byref(value))
    return value.available


def main():
    assert not request("http://127.0.0.1:11434/api/ps")["models"], "Existing Ollama session: leave it alone"
    queue = request("http://127.0.0.1:8188/queue")
    assert not queue["queue_running"] and not queue["queue_pending"], "ComfyUI busy"
    model = "llama3.1:8b-instruct-q4_K_M"
    assert any(m["name"] == model for m in request("http://127.0.0.1:11434/api/tags")["models"])
    assert available_ram() > 7 * 1024**3, "Insufficient currently available RAM for the CPU test"
    work = ROOT / "sviluppo/tests/artifacts" / ("live-narrative-" + str(uuid.uuid4())[:8])
    data = work / "data"; data.mkdir(parents=True)
    print("ARTIFACTS " + str(work), flush=True)
    (data / "settings.json").write_text(json.dumps({"comfy_url":"http://127.0.0.1:8188", "ollama_url":"http://127.0.0.1:11434",
        "llm_model":model,"role_llm_models":{"story":model},"ollama_execution":"cpu","legacy_sources":{}}), encoding="utf-8")
    reference = work / "synthetic-reference.png"; reference.write_bytes(png_bytes())
    port=free_port();base=f"http://127.0.0.1:{port}"
    log=(work/"server.log").open("w", encoding="utf-8")
    process=subprocess.Popen([str(ROOT/"app/nl-video-suite.exe"),"--port",str(port),"--data-dir",str(data)],cwd=ROOT,
        env={**os.environ,"NL_SUITE_ROOT":str(ROOT)},stdout=log,stderr=log)
    report={"synthetic_project":True,"real_inference":True,"review_required":True,"llm":model,"llm_execution":"cpu","work":str(work)}
    def api(tail,method="GET",body=None):return request(base+tail,method,body)
    def wait(job,timeout=1500):
        deadline=time.monotonic()+timeout;last=0;samples=[]
        while time.monotonic()<deadline:
            value=api("/api/jobs/"+job["id"])
            if time.monotonic()-last>20:
                last=time.monotonic();ram=round(available_ram()/1024**3,2)
                usage=subprocess.check_output(["nvidia-smi","--query-gpu=memory.used","--format=csv,noheader,nounits"],text=True).strip()
                samples.append({"available_ram_gib":ram,"gpu_used_mib":usage})
                print(value["kind"]+": "+value["message"]+f" | RAM libera {ram} GiB | GPU {usage} MiB",flush=True)
            if value["status"] in ("succeeded","failed"):
                value["memory_samples"]=samples
                assert value["status"]=="succeeded", value
                return value
            time.sleep(1)
        raise TimeoutError("Check the existing job before retrying: "+job["id"])
    try:
        for _ in range(100):
            try: api("/api/projects");break
            except OSError:time.sleep(.1)
        project=api("/api/projects","POST",{"title":"Collaudo locale reale · narrazione e Flux 2","aspect_ratio":"16:9"})
        report["project_id"]=project["id"];prefix="/api/projects/"+project["id"]
        wait(api(prefix+"/assets","POST",{"path":str(reference)}))
        image=api(prefix)["assets"][0]
        catalog=api("/api/workflows")
        ltx=next(w for w in catalog if w["name"]=="LTX-I2V-1280x720-NoAudio")
        ltx_path=ROOT/ltx["path"];before=hashlib.sha256(ltx_path.read_bytes()).hexdigest()
        doctor_body={"workflow_id":ltx["id"],"asset_inputs":{"167.image":image["id"]}}
        diagnosis=api("/api/workflows/diagnose","POST",doctor_body)
        selected=[r["id"] for r in diagnosis["repairs"] if r["id"]=="ltx_video_only_split:125" or any(c.get("rename_to") for c in r["changes"])]
        assert len(selected)==3, (selected,diagnosis)
        repaired=api("/api/workflows/repair","POST",{**doctor_body,"selected_repairs":selected})
        assert repaired["report"]["ready"],repaired["report"]
        assert hashlib.sha256(ltx_path.read_bytes()).hexdigest()==before
        report["ltx_live_repair"]={"selected":selected,"before":diagnosis,"after":repaired,"original_sha256":before,"neural_execution":False}
        print("PASS LTX: tre riparazioni reali su copia, reference esplicita, preflight pronto; nessuna generazione video pesante",flush=True)
        report["plan_job"]=wait(api(prefix+"/narrative/plan","POST",{"premise":"Un breve invito ad ascoltare il valore espressivo di una voce. Una sola scena illustrativa di un microfono da studio, senza persone, statistiche o crediti inventati.",
            "creative_brief":"Tono sobrio, cinematografico. Italiano naturale. Un solo soggetto, nessuna scritta o logo. Luce morbida, fondo scuro, accenti viola e rosa. La voce registrata sarà aggiunta in un secondo momento.","scene_count":1,"target_duration_ms":5000}))
        production=api(prefix+"/narrative");report["draft"]=production
        print("NARRAZIONE "+production["scenes"][0]["narration"],flush=True)
        # The synthetic test approves only its text to exercise generation.
        # Actual media are left unapproved, and no production timeline is written.
        flux=next(w for w in catalog if w["name"]=="image_flux2_text_to_image_9b")
        flux_path=ROOT/flux["path"];flux_hash=hashlib.sha256(flux_path.read_bytes()).hexdigest()
        production["scenes"][0]["visual_workflow"]={"workflow_id":flux["id"],"text_input":"75:74.text",
            "inputs":{"75:68.value":768,"75:69.value":448,"9.filename_prefix":"NL_Suite_Test_Flux2"},"asset_inputs":{}}
        production=api(prefix+"/narrative","PUT",{"production":production})
        production["script_approved"]=True;api(prefix+"/narrative","PUT",{"production":production})
        assert not request("http://127.0.0.1:11434/api/ps")["models"], "CPU LLM did not release before image generation"
        report["generation_job"]=wait(api(prefix+"/narrative/generate","POST",{"visuals":True,"voices":False}))
        production=api(prefix+"/narrative");project=api(prefix)
        scene=production["scenes"][0];assert scene["visual_asset_id"] and not scene["visual_approved"]
        asset=next(a for a in project["assets"] if a["id"]==scene["visual_asset_id"])
        report["output"]=str(data/"projects"/project["id"]/"assets"/asset["file_name"])
        assert project["timeline"]["clips"]==[]
        assert hashlib.sha256(flux_path.read_bytes()).hexdigest()==flux_hash
        report["flux_workflow_sha256"]=flux_hash;report["passed"]=True
        print("PASS real local narrative + Flux2; output awaits review: "+report["output"],flush=True)
    except Exception as error:
        report["passed"]=False;report["error"]=str(error)
        raise
    finally:
        (work/"report.json").write_text(json.dumps(report,ensure_ascii=False,indent=2),encoding="utf-8")
        process.terminate();process.wait(timeout=20);log.close()


if __name__=="__main__":main()
