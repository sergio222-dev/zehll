use chrono::{Datelike, Days, NaiveDate};

pub struct Day {
    pub date: NaiveDate,
    pub day: u32,
    pub in_month: bool,
}

/// Days in the month grid: six weeks.
pub const GRID_DAYS: u64 = 42;

/// Monday on or before `first`, where the month grid starts.
pub fn grid_start(first: NaiveDate) -> NaiveDate {
    first - Days::new(u64::from(first.weekday().num_days_from_monday()))
}

/// Six Monday-first weeks covering the month of `first`, padded with the adjacent months.
pub fn month_grid(first: NaiveDate) -> Vec<Day> {
    grid_start(first)
        .iter_days()
        .take(GRID_DAYS as usize)
        .map(|date| Day { date, day: date.day(), in_month: date.month() == first.month() })
        .collect()
}

/// ISO week number of each of the six rows of the month grid.
pub fn week_numbers(first: NaiveDate) -> Vec<u32> {
    grid_start(first).iter_weeks().take(GRID_DAYS as usize / 7).map(|monday| monday.iso_week().week()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(first: NaiveDate) -> (Vec<u32>, usize, Vec<u32>) {
        let grid = month_grid(first);
        let leading = grid.iter().take_while(|day| !day.in_month).map(|day| day.day).collect();
        let in_month = grid.iter().filter(|day| day.in_month).count();
        let trailing = grid.iter().rev().take_while(|day| !day.in_month).map(|day| day.day).collect::<Vec<_>>();
        (leading, in_month, trailing.into_iter().rev().collect())
    }

    #[test]
    fn november_2026_starts_on_a_sunday() {
        let first = NaiveDate::from_ymd_opt(2026, 11, 1).unwrap();
        assert_eq!(month_grid(first).len(), 42);
        assert_eq!(summary(first), (vec![26, 27, 28, 29, 30, 31], 30, vec![1, 2, 3, 4, 5, 6]));
    }

    #[test]
    fn month_starting_on_monday_has_no_leading_days() {
        let first = NaiveDate::from_ymd_opt(2026, 6, 1).unwrap();
        let (leading, in_month, trailing) = summary(first);
        assert!(leading.is_empty());
        assert_eq!(in_month, 30);
        assert_eq!(trailing.len(), 12);
    }

    #[test]
    fn grid_days_carry_their_dates_and_iso_weeks() {
        let first = NaiveDate::from_ymd_opt(2026, 11, 1).unwrap();
        let grid = month_grid(first);
        assert_eq!(grid[0].date, NaiveDate::from_ymd_opt(2026, 10, 26).unwrap());
        assert_eq!(grid[41].date, NaiveDate::from_ymd_opt(2026, 12, 6).unwrap());
        assert_eq!(week_numbers(first), [44, 45, 46, 47, 48, 49]);
        // December 2026 ends in ISO week 53, then January 2027 restarts at week 1.
        let december = NaiveDate::from_ymd_opt(2026, 12, 1).unwrap();
        assert_eq!(week_numbers(december), [49, 50, 51, 52, 53, 1]);
    }

    #[test]
    fn leap_february_and_year_boundary() {
        let february = NaiveDate::from_ymd_opt(2028, 2, 1).unwrap();
        assert_eq!(summary(february).1, 29);
        let january = NaiveDate::from_ymd_opt(2027, 1, 1).unwrap();
        assert_eq!(summary(january).0, vec![28, 29, 30, 31]);
    }
}
