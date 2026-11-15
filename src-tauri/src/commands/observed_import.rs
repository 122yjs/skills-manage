//! 관찰 항목 가져오기 (ADR-0002): 플랫폼 폴더의 내용을 보관함 원본으로 복사하고,
//! 그 폴더를 파일은 그대로 둔 채 관리 설치(복사 설치)로 기록한다.
use super::{
    linker,
    skill_duplicates::{self, DuplicateComparison},
};
use crate::{
    db::{self, DbPool},
    AppState,
};
use std::path::{Path, PathBuf};
use tauri::State;

/// 같은 이름의 보관함 스킬이 없으면 가져오고 `None`을 돌려준다. 있으면 아무것도 바꾸지 않고
/// 그 스킬과 비교한 결과를 돌려준다. 어느 쪽을 쓸지는 사용자가 고른다.
pub async fn import_observed_skill_impl(
    pool: &DbPool,
    skill_id: &str,
    agent_id: &str,
) -> Result<Option<DuplicateComparison>, String> {
    let observed = db::get_unmanaged_installations(pool, skill_id)
        .await?
        .into_iter()
        .find(|item| item.agent_id == agent_id)
        .ok_or_else(|| format!("가져올 관찰 항목이 없습니다: {skill_id} ({agent_id})"))?;
    let skill = db::get_skill_by_id(pool, skill_id)
        .await?
        .ok_or_else(|| format!("Skill '{skill_id}' not found"))?;

    let target = db::get_central_skills_dir(pool).await?.join(skill_id);
    let existing = skill
        .canonical_path
        .as_ref()
        .filter(|_| skill.is_central)
        .map(PathBuf::from)
        .or_else(|| target.symlink_metadata().is_ok().then(|| target.clone()));
    if let Some(existing) = existing {
        return Ok(Some(skill_duplicates::compare_paths(vec![
            observed.installed_path,
            existing.to_string_lossy().into_owned(),
        ])));
    }

    // 스킬 기록의 대표 경로가 다른 플랫폼 폴더일 수 있으므로, 누른 항목의 폴더에서 복사한다.
    linker::centralize_from(pool, skill, Path::new(&observed.installed_path), &target).await?;
    db::upsert_skill_installation(pool, &observed).await?;
    Ok(None)
}

#[tauri::command]
pub async fn import_observed_skill(
    state: State<'_, AppState>,
    skill_id: String,
    agent_id: String,
) -> Result<Option<DuplicateComparison>, String> {
    import_observed_skill_impl(&state.db, &skill_id, &agent_id).await
}
