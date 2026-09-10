/** 삭제 전 보존한 파일과 데이터베이스 백업의 복구 정보다. */
export interface RecoveryEntry {
  id: string;
  kind: "copy_backup" | "vault_trash" | "database";
  label: string;
  original_path: string;
  created_at: string;
  expires_at: string | null;
  backup_path: string;
}
