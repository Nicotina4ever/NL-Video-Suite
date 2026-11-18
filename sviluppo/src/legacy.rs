use crate::model::{AppConfig, Integration};

pub fn integrations(config: &AppConfig) -> Vec<Integration> {
    let definitions = [
        (
            "calliope",
            vec![
                "story → screenplay → shot decomposition".to_owned(),
                "role-tagged ComfyUI workflows".to_owned(),
                "project-scoped agent sessions".to_owned(),
                "film export".to_owned(),
            ],
            "Import SQLite in sola lettura, storyboard e workflow; scene 3D escluse, agente completo ancora da migrare",
        ),
        (
            "regista",
            vec![
                "multi-track reel timeline".to_owned(),
                "workflow diagnostics and deterministic repair".to_owned(),
                "asset generation and VRAM-aware queueing".to_owned(),
                "voice, music, image and video asset orchestration".to_owned(),
            ],
            "Import timeline e media, compositing, workflow e Workflow Doctor su copie; trasformazioni avanzate e riparazioni non deterministiche ancora da migrare",
        ),
        (
            "clipsai",
            vec![
                "WhisperX word-aligned transcript".to_owned(),
                "silence, breath, filler and bad-take cleanup".to_owned(),
                "social clip selection, SRT/ASS and vertical rendering".to_owned(),
            ],
            "Bridge attivo: trascrizione, sgrossatura, clip, reel, shorts e render con sottotitoli",
        ),
        (
            "zp_shorts",
            vec![
                "short candidate review and approval".to_owned(),
                "subtitle remapping".to_owned(),
                "vertical compilation".to_owned(),
            ],
            "Import candidati approvati con esclusioni KEEP dal motore originale; versioni e compilazioni avanzate ancora da migrare",
        ),
    ];

    definitions
        .into_iter()
        .map(|(id, capabilities, migration_status)| {
            let path = config.legacy_sources.get(id).cloned().unwrap_or_default();
            Integration {
                id: id.to_owned(),
                source_available: path.exists(),
                source_path: path.to_string_lossy().to_string(),
                capabilities,
                migration_status: migration_status.to_owned(),
            }
        })
        .collect()
}
