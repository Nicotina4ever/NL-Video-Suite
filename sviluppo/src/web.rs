use axum::{
    http::header,
    response::{Html, IntoResponse},
};
pub async fn index() -> Html<&'static str> {
    Html(include_str!("../web/index.html"))
}
pub async fn guide() -> Html<&'static str> {
    Html(include_str!("../../LEGGIMI.html"))
}
pub async fn appearance_javascript() -> impl IntoResponse {
    (
        [(
            header::CONTENT_TYPE,
            "application/javascript; charset=utf-8",
        )],
        include_str!("../web/appearance.js"),
    )
}
pub async fn appearance_stylesheet() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        include_str!("../web/appearance.css"),
    )
}
pub async fn nle() -> impl IntoResponse {
    (
        [(
            header::CONTENT_TYPE,
            "application/javascript; charset=utf-8",
        )],
        format!(
            "{}\n{}\n{}",
            include_str!("../web/clip_animation.js"),
            include_str!("../web/object_media.js"),
            include_str!("../web/nle.js")
        ),
    )
}
pub async fn nle_stylesheet() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        [
            include_str!("../web/nle.css"),
            include_str!("../web/editor_layout.css"),
            include_str!("../web/audio_mixer.css"),
        ]
        .join("\n"),
    )
}
pub async fn system_javascript() -> impl IntoResponse {
    (
        [(
            header::CONTENT_TYPE,
            "application/javascript; charset=utf-8",
        )],
        include_str!("../web/system.js"),
    )
}
pub async fn javascript() -> impl IntoResponse {
    (
        [(
            header::CONTENT_TYPE,
            "application/javascript; charset=utf-8",
        )],
        format!(
            "{}\n{}\n{}",
            [
                include_str!("../web/longform_analysis_ui.js"),
                include_str!("../web/longform_waveform.js"),
                include_str!("../web/longform_review.js"),
                include_str!("../web/longform_shots.js"),
                include_str!("../web/asr_manager.js"),
                include_str!("../web/longform.js"),
                include_str!("../web/shorts_studio.js"),
                include_str!("../web/subtitle_studio.js"),
                include_str!("../web/motion_typography.js"),
                include_str!("../web/motion_paths.js"),
                include_str!("../web/motion_svg.js"),
                include_str!("../web/motion_images.js"),
                include_str!("../web/motion_presets.js"),
                include_str!("../web/motion_groups.js"),
                include_str!("../web/motion_layer_timeline.js"),
                include_str!("../web/motion_ranking.js"),
                include_str!("../web/motion_idea.js"),
                include_str!("../web/motion_studio.js"),
                include_str!("../web/social_thumbnail.js"),
                include_str!("../web/social_advisor.js"),
                include_str!("../web/social_studio.js"),
                include_str!("../web/export_controls.js"),
                include_str!("../web/montage_rhythm.js"),
                include_str!("../web/montage_review.js"),
                include_str!("../web/montage_planner.js"),
                include_str!("../web/album.js"),
                include_str!("../web/album_script.js"),
                include_str!("../web/creative_montage.js")
            ]
            .join("\n"),
            [
                include_str!("../web/workflow_navigation.js"),
                include_str!("../web/workspace_flow.js"),
                include_str!("../web/editing_workspace.js"),
                include_str!("../web/project_management.js"),
                include_str!("../web/edit_versions.js"),
                include_str!("../web/audio_mixer.js"),
                include_str!("../web/audio_master_ui.js"),
                include_str!("../web/app.js")
            ]
            .join("\n"),
            [
                include_str!("../web/job_ui.js"),
                include_str!("../web/creative_ui.js"),
                include_str!("../web/resource_ui.js"),
                include_str!("../web/social_kit.js")
            ]
            .join("\n")
        ),
    )
}
pub async fn flow_javascript() -> impl IntoResponse {
    (
        [(
            header::CONTENT_TYPE,
            "application/javascript; charset=utf-8",
        )],
        include_str!("../web/flow.js"),
    )
}
pub async fn longform_stylesheet() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        include_str!("../web/longform.css"),
    )
}
pub async fn flow_stylesheet() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        include_str!("../web/flow.css"),
    )
}
pub async fn stylesheet() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        format!(
            "{}\n{}",
            include_str!("../web/style.css"),
            include_str!("../web/creative_montage.css")
        ),
    )
}
pub async fn narrative_javascript() -> impl IntoResponse {
    (
        [(
            header::CONTENT_TYPE,
            "application/javascript; charset=utf-8",
        )],
        format!(
            "{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
            include_str!("../web/narrative_preflight.js"),
            include_str!("../web/production_preset.js"),
            include_str!("../web/reference_ranges.js"),
            include_str!("../web/narrative_assistant.js"),
            include_str!("../web/narrative_library.js"),
            include_str!("../web/narrative_audio_first.js"),
            include_str!("../web/production_package.js"),
            include_str!("../web/narrative.js")
        ),
    )
}
pub async fn narrative_stylesheet() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        include_str!("../web/narrative.css"),
    )
}
