//! Предобработка событий ActivityWatch: flood и filter_period_intersect.
//!
//! Реализует pipeline аналогичный aw-server-rust:
//! 1. `flood` — заполняет микро-разрывы между heartbeat событиями
//! 2. `filter_period_intersect` — обрезает события по not-afk периодам

use chrono::{DateTime, Utc};

use super::models::{AfkStatus, AwAfkEvent, AwWindowEvent};

/// Пороговое время (секунды) для flood: gap меньше pulsetime заполняется
pub const DEFAULT_PULSETIME: f64 = 5.0;

/// Временной период (для filter_period_intersect)
#[derive(Debug, Clone)]
pub struct TimePeriod {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

/// Вычислить gap в секундах между концом e1 и началом e2
fn gap_secs(e1_end: DateTime<Utc>, e2_start: DateTime<Utc>) -> f64 {
    (e2_start - e1_end).num_milliseconds() as f64 / 1000.0
}

/// Создать DateTime из миллисекунд offset
fn dt_from_millis(base: DateTime<Utc>, offset_secs: f64) -> DateTime<Utc> {
    base + chrono::Duration::milliseconds((offset_secs * 1000.0) as i64)
}

// ==================== Flood ====================

/// Flood для window events: заполняет micro-gaps между heartbeat'ами.
///
/// Алгоритм (по мотивам aw-server-rust/flood.rs):
/// - Одинаковые data + gap < pulsetime → merge в одно событие
/// - Разные data + gap < pulsetime → gap делится 50/50
/// - gap >= pulsetime → оставить как есть
/// - Отрицательный gap + одинаковые data → merge (перекрытие heartbeats)
pub fn flood_window(events: Vec<AwWindowEvent>, pulsetime: f64) -> Vec<AwWindowEvent> {
    if events.len() <= 1 {
        return events;
    }

    let mut result: Vec<AwWindowEvent> = Vec::with_capacity(events.len());
    let mut iter = events.into_iter();
    let mut current = iter.next().unwrap();

    for next in iter {
        let gap = gap_secs(current.end_time(), next.timestamp);

        let same_data = current.app == next.app && current.title == next.title;

        if gap < 0.0 && same_data {
            // Отрицательный gap + одинаковые data → merge (перекрытие)
            let new_end = current.end_time().max(next.end_time());
            current.duration_secs =
                (new_end - current.timestamp).num_milliseconds() as f64 / 1000.0;
        } else if gap >= 0.0 && gap < pulsetime && same_data {
            // Одинаковые data, маленький gap → merge
            let new_end = next.end_time();
            current.duration_secs =
                (new_end - current.timestamp).num_milliseconds() as f64 / 1000.0;
        } else if gap > 0.0 && gap < pulsetime && !same_data {
            // Разные data, маленький gap → 50/50 split
            let half_gap = gap / 2.0;
            current.duration_secs += half_gap;
            let new_start = dt_from_millis(next.timestamp, -half_gap);
            let added_duration = (next.timestamp - new_start).num_milliseconds() as f64 / 1000.0;
            result.push(current);
            current = AwWindowEvent {
                timestamp: new_start,
                duration_secs: next.duration_secs + added_duration,
                app: next.app,
                title: next.title,
            };
        } else {
            // gap >= pulsetime или отрицательный gap с разными data → оставить как есть
            result.push(current);
            current = next;
        }
    }
    result.push(current);

    result
}

/// Flood для AFK events: заполняет micro-gaps между heartbeat'ами.
///
/// Аналогичен flood_window, но "data" = status (Afk/NotAfk).
pub fn flood_afk(events: Vec<AwAfkEvent>, pulsetime: f64) -> Vec<AwAfkEvent> {
    if events.len() <= 1 {
        return events;
    }

    let mut result: Vec<AwAfkEvent> = Vec::with_capacity(events.len());
    let mut iter = events.into_iter();
    let mut current = iter.next().unwrap();

    for next in iter {
        let gap = gap_secs(current.end_time(), next.timestamp);

        let same_data = current.status == next.status;

        if gap < 0.0 && same_data {
            let new_end = current.end_time().max(next.end_time());
            current.duration_secs =
                (new_end - current.timestamp).num_milliseconds() as f64 / 1000.0;
        } else if gap >= 0.0 && gap < pulsetime && same_data {
            let new_end = next.end_time();
            current.duration_secs =
                (new_end - current.timestamp).num_milliseconds() as f64 / 1000.0;
        } else if gap > 0.0 && gap < pulsetime && !same_data {
            let half_gap = gap / 2.0;
            current.duration_secs += half_gap;
            let new_start = dt_from_millis(next.timestamp, -half_gap);
            let added_duration = (next.timestamp - new_start).num_milliseconds() as f64 / 1000.0;
            result.push(current);
            current = AwAfkEvent {
                timestamp: new_start,
                duration_secs: next.duration_secs + added_duration,
                status: next.status,
            };
        } else {
            result.push(current);
            current = next;
        }
    }
    result.push(current);

    result
}

// ==================== Filter Period Intersect ====================

/// Извлечь not-afk периоды из AFK событий.
///
/// Возвращает только периоды со статусом NotAfk.
pub fn extract_not_afk_periods(afk_events: &[AwAfkEvent]) -> Vec<TimePeriod> {
    afk_events
        .iter()
        .filter(|e| e.status == AfkStatus::NotAfk)
        .map(|e| TimePeriod {
            start: e.timestamp,
            end: e.end_time(),
        })
        .collect()
}

/// Обрезать window events по периодам (two-pointer O(n+m)).
///
/// Одно событие может быть разрезано на несколько, если перекрывает
/// несколько not-afk периодов.
pub fn filter_period_intersect(
    events: &[AwWindowEvent],
    periods: &[TimePeriod],
) -> Vec<AwWindowEvent> {
    if events.is_empty() || periods.is_empty() {
        return Vec::new();
    }

    let mut result = Vec::new();
    let mut ei = 0; // указатель по событиям
    let mut pi = 0; // указатель по периодам

    // Для каждого события храним "оставшуюся" часть
    let mut current_event_start: Option<DateTime<Utc>> = None;
    let mut current_event_end: Option<DateTime<Utc>> = None;
    let mut current_event_app: Option<&str> = None;
    let mut current_event_title: Option<&str> = None;

    // Инициализируем первое событие
    if let Some(e) = events.first() {
        current_event_start = Some(e.timestamp);
        current_event_end = Some(e.end_time());
        current_event_app = Some(&e.app);
        current_event_title = Some(&e.title);
    }

    while ei < events.len() && pi < periods.len() {
        let e_start = current_event_start.unwrap();
        let e_end = current_event_end.unwrap();
        let app = current_event_app.unwrap();
        let title = current_event_title.unwrap();

        let p_start = periods[pi].start;
        let p_end = periods[pi].end;

        // Нет пересечения: событие полностью до периода
        if e_end <= p_start {
            ei += 1;
            if ei < events.len() {
                current_event_start = Some(events[ei].timestamp);
                current_event_end = Some(events[ei].end_time());
                current_event_app = Some(&events[ei].app);
                current_event_title = Some(&events[ei].title);
            }
            continue;
        }

        // Нет пересечения: период полностью до события
        if p_end <= e_start {
            pi += 1;
            continue;
        }

        // Есть пересечение — вычисляем intersection
        let inter_start = e_start.max(p_start);
        let inter_end = e_end.min(p_end);
        let duration = (inter_end - inter_start).num_milliseconds() as f64 / 1000.0;

        if duration > 0.0 {
            result.push(AwWindowEvent {
                timestamp: inter_start,
                duration_secs: duration,
                app: app.to_string(),
                title: title.to_string(),
            });
        }

        // Продвигаем указатель с меньшим end
        if e_end <= p_end {
            // Событие закончилось — берём следующее
            ei += 1;
            if ei < events.len() {
                current_event_start = Some(events[ei].timestamp);
                current_event_end = Some(events[ei].end_time());
                current_event_app = Some(&events[ei].app);
                current_event_title = Some(&events[ei].title);
            }
        } else {
            // Период закончился — остаток события переносим
            current_event_start = Some(p_end);
            // current_event_end, app, title остаются
            pi += 1;
        }
    }

    result
}

// ==================== Convenience ====================

/// Предобработка: flood обоих потоков → extract not-afk → filter_period_intersect.
///
/// Возвращает тройку:
/// - `active_window` — пересечение с not-afk (для collect_browse_stats)
/// - `flooded_window` — flood без пересечения (для correlate_session, collect_terminal_focus_stats)
/// - `flooded_afk` — flooded AFK (для функций с ручной обработкой AFK)
pub fn preprocess_active_window_events(
    window_events: Vec<AwWindowEvent>,
    afk_events: Vec<AwAfkEvent>,
    pulsetime: f64,
) -> (Vec<AwWindowEvent>, Vec<AwWindowEvent>, Vec<AwAfkEvent>) {
    let flooded_window = flood_window(window_events, pulsetime);
    let flooded_afk = flood_afk(afk_events, pulsetime);

    let not_afk_periods = extract_not_afk_periods(&flooded_afk);
    let active_window = filter_period_intersect(&flooded_window, &not_afk_periods);

    (active_window, flooded_window, flooded_afk)
}

// ==================== Тесты ====================

#[cfg(test)]
mod tests {
    use super::*;

    /// Базовый timestamp для тестов: 2026-01-01T10:00:00Z
    fn base_ts() -> DateTime<Utc> {
        "2026-01-01T10:00:00Z".parse().unwrap()
    }

    /// Хелпер: создать AwWindowEvent
    fn win(ts_secs: i64, dur: f64, app: &str, title: &str) -> AwWindowEvent {
        AwWindowEvent {
            timestamp: base_ts() + chrono::Duration::seconds(ts_secs),
            duration_secs: dur,
            app: app.to_string(),
            title: title.to_string(),
        }
    }

    /// Хелпер: создать AwAfkEvent
    fn afk(ts_secs: i64, dur: f64, status: AfkStatus) -> AwAfkEvent {
        AwAfkEvent {
            timestamp: base_ts() + chrono::Duration::seconds(ts_secs),
            duration_secs: dur,
            status,
        }
    }

    // ==================== flood_window ====================

    #[test]
    fn flood_window_empty_input() {
        let result = flood_window(vec![], 5.0);
        assert!(result.is_empty());
    }

    #[test]
    fn flood_window_single_event() {
        let events = vec![win(0, 1.0, "Code", "main.rs")];
        let result = flood_window(events, 5.0);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].duration_secs, 1.0);
    }

    #[test]
    fn flood_window_same_data_small_gap() {
        // Два одинаковых события с gap 2s (< pulsetime 5s) → merge
        let events = vec![
            win(0, 1.0, "Code", "main.rs"),  // 0..1
            win(3, 1.0, "Code", "main.rs"),   // 3..4, gap=2s
        ];
        let result = flood_window(events, 5.0);
        assert_eq!(result.len(), 1);
        // Merged: 0..4 = 4s
        assert!((result[0].duration_secs - 4.0).abs() < 0.01);
    }

    #[test]
    fn flood_window_same_data_chain() {
        // 5 одинаковых подряд с gap 1s → одно событие
        let events = vec![
            win(0, 1.0, "Code", "main.rs"),   // 0..1
            win(2, 1.0, "Code", "main.rs"),   // 2..3
            win(4, 1.0, "Code", "main.rs"),   // 4..5
            win(6, 1.0, "Code", "main.rs"),   // 6..7
            win(8, 1.0, "Code", "main.rs"),   // 8..9
        ];
        let result = flood_window(events, 5.0);
        assert_eq!(result.len(), 1);
        // Merged: 0..9 = 9s
        assert!((result[0].duration_secs - 9.0).abs() < 0.01);
    }

    #[test]
    fn flood_window_same_data_adjacent() {
        // gap == 0 → merge
        let events = vec![
            win(0, 2.0, "Code", "main.rs"),   // 0..2
            win(2, 3.0, "Code", "main.rs"),   // 2..5
        ];
        let result = flood_window(events, 5.0);
        assert_eq!(result.len(), 1);
        assert!((result[0].duration_secs - 5.0).abs() < 0.01);
    }

    #[test]
    fn flood_window_different_data_small_gap() {
        // Два разных с gap 2s (< pulsetime 5s) → gap 50/50
        let events = vec![
            win(0, 1.0, "Code", "main.rs"),     // 0..1
            win(3, 1.0, "Firefox", "google.com"), // 3..4, gap=2s
        ];
        let result = flood_window(events, 5.0);
        assert_eq!(result.len(), 2);
        // Первый: duration увеличен на 1s (half of 2s gap)
        assert!((result[0].duration_secs - 2.0).abs() < 0.01);
        // Второй: start сдвинут на 1s назад, duration увеличен на 1s
        assert!((result[1].duration_secs - 2.0).abs() < 0.01);
    }

    #[test]
    fn flood_window_different_data_large_gap() {
        // gap >= pulsetime → без изменений
        let events = vec![
            win(0, 1.0, "Code", "main.rs"),       // 0..1
            win(10, 1.0, "Firefox", "google.com"), // 10..11, gap=9s
        ];
        let result = flood_window(events, 5.0);
        assert_eq!(result.len(), 2);
        assert!((result[0].duration_secs - 1.0).abs() < 0.01);
        assert!((result[1].duration_secs - 1.0).abs() < 0.01);
    }

    #[test]
    fn flood_window_mixed_aab() {
        // A, A, B → AA merged + B = 2 события
        let events = vec![
            win(0, 1.0, "Code", "main.rs"),      // 0..1
            win(2, 1.0, "Code", "main.rs"),       // 2..3, gap=1s → merge
            win(5, 1.0, "Firefox", "google.com"),  // 5..6, gap=2s → 50/50
        ];
        let result = flood_window(events, 5.0);
        assert_eq!(result.len(), 2);
        // Первый: merged AA (0..3) + half gap (1s) = 4s
        assert!((result[0].duration_secs - 4.0).abs() < 0.01);
        // Второй: start сдвинут на 1s назад = 2s
        assert!((result[1].duration_secs - 2.0).abs() < 0.01);
    }

    #[test]
    fn flood_window_negative_gap_same_data() {
        // Перекрытие + одинаковые → merge
        let events = vec![
            win(0, 3.0, "Code", "main.rs"),   // 0..3
            win(2, 3.0, "Code", "main.rs"),   // 2..5, overlap=1s
        ];
        let result = flood_window(events, 5.0);
        assert_eq!(result.len(), 1);
        // Merged: 0..5 = 5s
        assert!((result[0].duration_secs - 5.0).abs() < 0.01);
    }

    #[test]
    fn flood_window_negative_gap_different_data() {
        // Перекрытие + разные → оставить как есть
        let events = vec![
            win(0, 3.0, "Code", "main.rs"),      // 0..3
            win(2, 3.0, "Firefox", "google.com"), // 2..5, overlap
        ];
        let result = flood_window(events, 5.0);
        assert_eq!(result.len(), 2);
        assert!((result[0].duration_secs - 3.0).abs() < 0.01);
        assert!((result[1].duration_secs - 3.0).abs() < 0.01);
    }

    #[test]
    fn flood_window_zero_duration_events() {
        // Не должно паниковать
        let events = vec![
            win(0, 0.0, "Code", "main.rs"),
            win(1, 0.0, "Code", "main.rs"),
        ];
        let result = flood_window(events, 5.0);
        assert!(!result.is_empty());
    }

    // ==================== flood_afk ====================

    #[test]
    fn flood_afk_merge_same_status() {
        // NotAfk + NotAfk с маленьким gap → merge
        let events = vec![
            afk(0, 10.0, AfkStatus::NotAfk),   // 0..10
            afk(12, 10.0, AfkStatus::NotAfk),  // 12..22, gap=2s
        ];
        let result = flood_afk(events, 5.0);
        assert_eq!(result.len(), 1);
        // Merged: 0..22 = 22s
        assert!((result[0].duration_secs - 22.0).abs() < 0.01);
    }

    #[test]
    fn flood_afk_different_status() {
        // NotAfk + Afk с маленьким gap → gap 50/50
        let events = vec![
            afk(0, 10.0, AfkStatus::NotAfk), // 0..10
            afk(12, 10.0, AfkStatus::Afk),   // 12..22, gap=2s
        ];
        let result = flood_afk(events, 5.0);
        assert_eq!(result.len(), 2);
        // Первый: duration + 1s = 11s
        assert!((result[0].duration_secs - 11.0).abs() < 0.01);
        // Второй: start сдвинут на 1s, duration + 1s = 11s
        assert!((result[1].duration_secs - 11.0).abs() < 0.01);
    }

    // ==================== extract_not_afk_periods ====================

    #[test]
    fn extract_not_afk_all_not_afk() {
        let events = vec![
            afk(0, 10.0, AfkStatus::NotAfk),
            afk(10, 20.0, AfkStatus::NotAfk),
        ];
        let periods = extract_not_afk_periods(&events);
        assert_eq!(periods.len(), 2);
    }

    #[test]
    fn extract_not_afk_all_afk() {
        let events = vec![
            afk(0, 10.0, AfkStatus::Afk),
            afk(10, 20.0, AfkStatus::Afk),
        ];
        let periods = extract_not_afk_periods(&events);
        assert!(periods.is_empty());
    }

    #[test]
    fn extract_not_afk_mixed() {
        let events = vec![
            afk(0, 10.0, AfkStatus::NotAfk),
            afk(10, 20.0, AfkStatus::Afk),
            afk(30, 10.0, AfkStatus::NotAfk),
        ];
        let periods = extract_not_afk_periods(&events);
        assert_eq!(periods.len(), 2);
        // Первый период: 0..10
        assert_eq!(periods[0].start, base_ts());
        assert_eq!(
            periods[0].end,
            base_ts() + chrono::Duration::seconds(10)
        );
        // Второй период: 30..40
        assert_eq!(
            periods[1].start,
            base_ts() + chrono::Duration::seconds(30)
        );
        assert_eq!(
            periods[1].end,
            base_ts() + chrono::Duration::seconds(40)
        );
    }

    // ==================== filter_period_intersect ====================

    #[test]
    fn filter_intersect_empty_events() {
        let periods = vec![TimePeriod {
            start: base_ts(),
            end: base_ts() + chrono::Duration::seconds(10),
        }];
        let result = filter_period_intersect(&[], &periods);
        assert!(result.is_empty());
    }

    #[test]
    fn filter_intersect_empty_periods() {
        let events = vec![win(0, 5.0, "Code", "main.rs")];
        let result = filter_period_intersect(&events, &[]);
        assert!(result.is_empty());
    }

    #[test]
    fn filter_intersect_full_overlap() {
        // Событие полностью внутри периода → без изменений
        let events = vec![win(2, 3.0, "Code", "main.rs")]; // 2..5
        let periods = vec![TimePeriod {
            start: base_ts(),
            end: base_ts() + chrono::Duration::seconds(10),
        }]; // 0..10
        let result = filter_period_intersect(&events, &periods);
        assert_eq!(result.len(), 1);
        assert!((result[0].duration_secs - 3.0).abs() < 0.01);
    }

    #[test]
    fn filter_intersect_partial_overlap_start() {
        // Событие начинается до периода → обрезка начала
        let events = vec![win(0, 10.0, "Code", "main.rs")]; // 0..10
        let periods = vec![TimePeriod {
            start: base_ts() + chrono::Duration::seconds(5),
            end: base_ts() + chrono::Duration::seconds(15),
        }]; // 5..15
        let result = filter_period_intersect(&events, &periods);
        assert_eq!(result.len(), 1);
        // intersection: 5..10 = 5s
        assert!((result[0].duration_secs - 5.0).abs() < 0.01);
        assert_eq!(
            result[0].timestamp,
            base_ts() + chrono::Duration::seconds(5)
        );
    }

    #[test]
    fn filter_intersect_partial_overlap_end() {
        // Событие заканчивается после периода → обрезка конца
        let events = vec![win(5, 10.0, "Code", "main.rs")]; // 5..15
        let periods = vec![TimePeriod {
            start: base_ts(),
            end: base_ts() + chrono::Duration::seconds(10),
        }]; // 0..10
        let result = filter_period_intersect(&events, &periods);
        assert_eq!(result.len(), 1);
        // intersection: 5..10 = 5s
        assert!((result[0].duration_secs - 5.0).abs() < 0.01);
    }

    #[test]
    fn filter_intersect_event_split_two_periods() {
        // Одно событие перекрывает два периода → разрезается на два
        let events = vec![win(0, 30.0, "Code", "main.rs")]; // 0..30
        let periods = vec![
            TimePeriod {
                start: base_ts(),
                end: base_ts() + chrono::Duration::seconds(10),
            }, // 0..10
            TimePeriod {
                start: base_ts() + chrono::Duration::seconds(20),
                end: base_ts() + chrono::Duration::seconds(30),
            }, // 20..30
        ];
        let result = filter_period_intersect(&events, &periods);
        assert_eq!(result.len(), 2);
        // Первый кусок: 0..10 = 10s
        assert!((result[0].duration_secs - 10.0).abs() < 0.01);
        // Второй кусок: 20..30 = 10s
        assert!((result[1].duration_secs - 10.0).abs() < 0.01);
    }

    #[test]
    fn filter_intersect_no_overlap() {
        let events = vec![win(0, 5.0, "Code", "main.rs")]; // 0..5
        let periods = vec![TimePeriod {
            start: base_ts() + chrono::Duration::seconds(10),
            end: base_ts() + chrono::Duration::seconds(20),
        }]; // 10..20
        let result = filter_period_intersect(&events, &periods);
        assert!(result.is_empty());
    }

    #[test]
    fn filter_intersect_multiple_events_one_period() {
        let events = vec![
            win(0, 5.0, "Code", "main.rs"),      // 0..5 — вне периода
            win(8, 5.0, "Code", "main.rs"),      // 8..13 — частично
            win(15, 3.0, "Firefox", "google.com"), // 15..18 — полностью внутри
            win(25, 5.0, "Code", "main.rs"),     // 25..30 — вне периода
        ];
        let periods = vec![TimePeriod {
            start: base_ts() + chrono::Duration::seconds(10),
            end: base_ts() + chrono::Duration::seconds(20),
        }]; // 10..20
        let result = filter_period_intersect(&events, &periods);
        assert_eq!(result.len(), 2);
        // Первый: 10..13 = 3s (обрезанный)
        assert!((result[0].duration_secs - 3.0).abs() < 0.01);
        assert_eq!(
            result[0].timestamp,
            base_ts() + chrono::Duration::seconds(10)
        );
        // Второй: 15..18 = 3s (полностью внутри)
        assert!((result[1].duration_secs - 3.0).abs() < 0.01);
    }

    #[test]
    fn filter_intersect_zero_duration_event() {
        // Zero duration → пропуск (duration == 0)
        let events = vec![win(5, 0.0, "Code", "main.rs")]; // 5..5
        let periods = vec![TimePeriod {
            start: base_ts(),
            end: base_ts() + chrono::Duration::seconds(10),
        }];
        let result = filter_period_intersect(&events, &periods);
        // Zero duration → duration=0, не добавляется (> 0.0 check)
        assert!(result.is_empty());
    }

    // ==================== Интеграционный тест ====================

    #[test]
    fn realistic_scenario() {
        // Симулируем 600 heartbeat events (1s каждый, gap 1s) + AFK данные
        // Период: 0..1200s (600 heartbeats x 2s каждый)
        // AFK: not-afk 0..300, afk 300..600, not-afk 600..1200
        let mut window_events = Vec::new();
        for i in 0..600 {
            let ts = i * 2; // каждые 2 секунды
            window_events.push(win(ts as i64, 1.0, "Code", "main.rs"));
        }

        let afk_events = vec![
            afk(0, 300.0, AfkStatus::NotAfk),    // 0..300
            afk(300, 300.0, AfkStatus::Afk),      // 300..600
            afk(600, 600.0, AfkStatus::NotAfk),   // 600..1200
        ];

        let (active, flooded_win, flooded_afk_result) =
            preprocess_active_window_events(window_events, afk_events, DEFAULT_PULSETIME);

        // flooded window: все 600 events с gap=1s (<5s) и одинаковые данные → 1 событие
        assert_eq!(flooded_win.len(), 1);
        // 0..1199 = 1199s
        assert!((flooded_win[0].duration_secs - 1199.0).abs() < 1.0);

        // flooded AFK: 3 события (разные статусы, gap=0)
        assert_eq!(flooded_afk_result.len(), 3);

        // active window: intersection с not-afk (0..300 и 600..1200)
        // Одно flooded событие (0..1199) пересекается с двумя not-afk периодами
        assert_eq!(active.len(), 2);

        // Первый кусок: 0..300 = 300s
        let active_total: f64 = active.iter().map(|e| e.duration_secs).sum();
        // 300 + (1199-600) = 300 + 599 = 899s
        assert!((active_total - 899.0).abs() < 1.0);
    }
}
