use common::WallpaperItem;
use library::Database;

#[test]
fn test_database_migrations_and_wallpapers_crud() {
    let db = Database::open_in_memory().expect("open_in_memory failed");

    let wp = WallpaperItem {
        id: "wp_test_1".to_string(),
        name: "Test Wallpaper".to_string(),
        kind: "video".to_string(),
        source_path: "C:/test.mp4".to_string(),
        processed_path: "C:/test.mp4".to_string(),
        thumb_path: "C:/test_thumb.webp".to_string(),
        fit: "fill".to_string(),
        fps_cap: Some(30),
        width: Some(1920),
        height: Some(1080),
        duration_ms: Some(15000),
        created_at: 1700000000,
    };

    db.save_wallpaper(&wp).expect("save_wallpaper failed");
    let list = db.list_wallpapers().expect("list_wallpapers failed");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, "wp_test_1");
    assert_eq!(list[0].kind, "video");

    db.set_kv("active_id", "wp_test_1").expect("set_kv failed");
    assert_eq!(
        db.get_kv("active_id").expect("get_kv failed"),
        Some("wp_test_1".to_string())
    );

    db.delete_wallpaper("wp_test_1").expect("delete_wallpaper failed");
    let list_after = db.list_wallpapers().expect("list_wallpapers failed");
    assert_eq!(list_after.len(), 0);
}
