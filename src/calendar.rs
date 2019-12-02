//! Implementation of (bank) holidays.
//! Calendars are required to verify whether an exchange is open or if a certain
//! cash flow could be settled on a specific day. They are also needed to calculate 
//! the amount of business days between to given dates.
//! Because of the settlement rules, bank holidays have an impact on how to 
//! rollout cash flows from fixed income products.
//! The approach taken here is to define a set of rules to determine bank holidays.
//! From this set of rules, a calendar is generated by calculating all bank holidays
//! within a given range of years for fast access. 

use chrono::{Datelike, Duration, NaiveDate, Weekday};
use std::collections::BTreeSet;
extern crate computus;

pub enum NthWeekday {
    First,
    Second,
    Third,
    Fourth,
    Last,
}

pub enum Holiday {
    /// Though weekends are no holidays, they need to be specified in the calendar. Weekends are assumed to be non-business days.
    /// In most countries, weekends include Saturday (`Sat`) and Sunday (`Sun`). Unfortunately, there are a few exceptions.
    WeekDay(Weekday),
    /// A holiday that occurs every year on the same day.
    /// `first` and `last` are the first and last year this day is a holiday (inclusively).
    YearlyDay {
        month: u32,
        day: u32,
        first: Option<i32>,
        last: Option<i32>,
    },
    /// Occurs every year, but is moved to next non-weekend day if it falls on a weekday.
    /// Note that Saturday and Sunday here assumed to be weekend days, even if these days
    /// are not defined as weekends in this calendar. If the next Monday is already a holiday,
    /// the date will be moved to the next available business day.
    /// `first` and `last` are the first and last year this day is a holiday (inclusively).
    MovableYearlyDay {
        month: u32,
        day: u32,
        first: Option<i32>,
        last: Option<i32>,
    },
    /// A single holiday which is valid only once in time.
    SingularDay(NaiveDate),
    /// A holiday that is defined in relative days (e.g. -2 for Good Friday) to Easter (Sunday).
    EasterOffset(i32),
    /// A holiday that falls on the nth (or last) weekday of a specific month, e.g. the first Monday in May.
    /// `first` and `last` are the first and last year this day is a holiday (inclusively).
    MonthWeekday {
        month: u32,
        weekday: Weekday,
        nth: NthWeekday,
        first: Option<i32>,
        last: Option<i32>,
    },
}

/// Calendar for arbitrary complex holiday rules
#[derive(Debug, Clone)]
pub struct Calendar {
    holidays: BTreeSet<NaiveDate>,
    weekdays: Vec<Weekday>,
}

impl Calendar {
    /// Calculate all holidays and recognize weekend days for a given range of years 
    /// from `start` to `end` (inclusively). The calculation is performed on the basis
    /// of a vector of holiday rules.
    pub fn calc_calendar(holiday_rules: &Vec<Holiday>, start: i32, end: i32) -> Calendar {
        let mut holidays = BTreeSet::new();
        let mut weekdays = Vec::new();

        for rule in holiday_rules {
            match rule {
                Holiday::SingularDay(date) => {
                    let year = date.year();
                    if year >= start && year <= end {
                        holidays.insert(date.clone());
                    }
                }
                Holiday::WeekDay(weekday) => {
                    weekdays.push(weekday.clone());
                }
                Holiday::YearlyDay {
                    month,
                    day,
                    first,
                    last,
                } => {
                    let (first, last) = Self::calc_first_and_last(start, end, first, last);
                    for year in first..last + 1 {
                        holidays.insert(NaiveDate::from_ymd(year, *month, *day));
                    }
                }
                Holiday::MovableYearlyDay {
                    month,
                    day,
                    first,
                    last,
                } => {
                    let (first, last) = Self::calc_first_and_last(start, end, first, last);
                    for year in first..last + 1 {
                        let date = NaiveDate::from_ymd(year, *month, *day);
                        // must not fall on weekend, but also not on another holiday! (not yet implemented)
                        let mut date = match date.weekday() {
                            Weekday::Sat => date.succ().succ(),
                            Weekday::Sun => date.succ(),
                            _ => date,
                        };
                        while holidays.get(&date).is_some() {
                            date = date.succ();
                        }
                        holidays.insert(date);
                    }
                }
                Holiday::EasterOffset(offset) => {
                    for year in start..end + 1 {
                        let easter = computus::gregorian(year).unwrap();
                        let easter = NaiveDate::from_ymd(easter.year, easter.month, easter.day);
                        let date = easter
                            .checked_add_signed(Duration::days(*offset as i64))
                            .unwrap();
                        holidays.insert(date);
                    }
                }
                Holiday::MonthWeekday {
                    month,
                    weekday,
                    nth,
                    first,
                    last
                } => {
                    let (first, last) = Self::calc_first_and_last(start, end, first, last);
                    for year in first..last + 1 {
                        let day = match nth {
                            NthWeekday::First => 1,
                            NthWeekday::Second => 8,
                            NthWeekday::Third => 15,
                            NthWeekday::Fourth => 22,
                            NthWeekday::Last => last_day_of_month(year, *month),
                        };
                        let mut date = NaiveDate::from_ymd(year, *month, day);
                        while date.weekday() != *weekday {
                            date = match nth {
                                NthWeekday::Last => date.pred(),
                                _ => date.succ(),
                            }
                        }
                        holidays.insert(date);
                    }
                }
            }
        }
        Calendar {
            holidays: holidays,
            weekdays: weekdays,
        }
    }

    /// Calculate the next business day
    pub fn next_bday(&self, mut date: NaiveDate) -> NaiveDate {
        date = date.succ();
        while !self.is_business_day(date) {
            date = date.succ();
        }
        date
    }

    /// Calculate the previous business day
    pub fn prev_bday(&self, mut date: NaiveDate) -> NaiveDate {
        date = date.pred();
        while !self.is_business_day(date) {
            date = date.pred();
        }
        date
    }

    fn calc_first_and_last(
        start: i32,
        end: i32,
        first: &Option<i32>,
        last: &Option<i32>,
    ) -> (i32, i32) {
        let first = match first {
            Some(year) => std::cmp::max(start, *year),
            _ => start,
        };
        let last = match last {
            Some(year) => std::cmp::min(end, *year),
            _ => end,
        };
        (first, last)
    }

    /// Returns true if the date falls on a weekend
    pub fn is_weekend(&self, day: NaiveDate) -> bool {
        let weekday = day.weekday();
        for w_day in &self.weekdays {
            if weekday == *w_day {
                return true;
            }
        }
        false
    }

    /// Returns true if the specified day is a bank holiday
    pub fn is_holiday(&self, date: NaiveDate) -> bool {
        self.holidays.get(&date).is_some()
    }

    /// Returns true if the specified day is a business day
    pub fn is_business_day(&self, date: NaiveDate) -> bool {
        !self.is_weekend(date) && !self.is_holiday(date)
    }
}

/// Returns true if the specified year is a leap year (i.e. Feb 29th exists for this year)
pub fn is_leap_year(year: i32) -> bool {
    NaiveDate::from_ymd_opt(year, 2, 29).is_some()
}

/// Calculate the last day of a given month in a given year
pub fn last_day_of_month(year: i32, month: u32) -> u32 {
    NaiveDate::from_ymd_opt(year, month + 1, 1)
        .unwrap_or(NaiveDate::from_ymd(year + 1, 1, 1))
        .pred()
        .day()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_dates_calendar() {
        let holidays = vec![
            Holiday::SingularDay(NaiveDate::from_ymd(2019, 11, 20)),
            Holiday::SingularDay(NaiveDate::from_ymd(2019, 11, 24)),
            Holiday::SingularDay(NaiveDate::from_ymd(2019, 11, 25)),
            Holiday::WeekDay(Weekday::Sat),
            Holiday::WeekDay(Weekday::Sun),
        ];
        let cal = Calendar::calc_calendar(&holidays, 2019, 2019);

        assert_eq!(false, cal.is_business_day(NaiveDate::from_ymd(2019, 11, 20)));
        assert_eq!(true, cal.is_business_day(NaiveDate::from_ymd(2019, 11, 21)));
        assert_eq!(true, cal.is_business_day(NaiveDate::from_ymd(2019, 11, 22)));
        // weekend
        assert_eq!(false, cal.is_business_day(NaiveDate::from_ymd(2019, 11, 23)));
        assert_eq!(true, cal.is_weekend(NaiveDate::from_ymd(2019, 11, 23)));
        assert_eq!(false, cal.is_holiday(NaiveDate::from_ymd(2019, 11, 23)));
        // weekend and holiday
        assert_eq!(false, cal.is_business_day(NaiveDate::from_ymd(2019, 11, 24)));
        assert_eq!(true, cal.is_weekend(NaiveDate::from_ymd(2019, 11, 24)));
        assert_eq!(true, cal.is_holiday(NaiveDate::from_ymd(2019, 11, 24)));
        assert_eq!(false, cal.is_business_day(NaiveDate::from_ymd(2019, 11, 25)));
        assert_eq!(true, cal.is_business_day(NaiveDate::from_ymd(2019, 11, 26)));
    }

    #[test]
    fn test_yearly_day() {        
        let holidays = vec![
            Holiday::YearlyDay{month: 11, day: 1, first: None, last: None},
            Holiday::YearlyDay{month: 11, day: 2, first: Some(2019), last: None},
            Holiday::YearlyDay{month: 11, day: 3, first: None, last: Some(2019)},
            Holiday::YearlyDay{month: 11, day: 4, first: Some(2019), last: Some(2019)},
        ];
        let cal = Calendar::calc_calendar(&holidays, 2018, 2020);
        
        assert_eq!(true, cal.is_holiday(NaiveDate::from_ymd(2018, 11, 1)));
        assert_eq!(true, cal.is_holiday(NaiveDate::from_ymd(2019, 11, 1)));
        assert_eq!(true, cal.is_holiday(NaiveDate::from_ymd(2020, 11, 1)));

        assert_eq!(false, cal.is_holiday(NaiveDate::from_ymd(2018, 11, 2)));
        assert_eq!(true, cal.is_holiday(NaiveDate::from_ymd(2019, 11, 2)));
        assert_eq!(true, cal.is_holiday(NaiveDate::from_ymd(2020, 11, 2)));

        assert_eq!(true, cal.is_holiday(NaiveDate::from_ymd(2018, 11, 3)));
        assert_eq!(true, cal.is_holiday(NaiveDate::from_ymd(2019, 11, 3)));
        assert_eq!(false, cal.is_holiday(NaiveDate::from_ymd(2020, 11, 3)));

        assert_eq!(false, cal.is_holiday(NaiveDate::from_ymd(2018, 11, 4)));
        assert_eq!(true, cal.is_holiday(NaiveDate::from_ymd(2019, 11, 4)));
        assert_eq!(false, cal.is_holiday(NaiveDate::from_ymd(2020, 11, 4)));
    }
    
    #[test]
    fn test_movable_yearly_day() {        
        let holidays = vec![
            Holiday::MovableYearlyDay{month: 11, day: 1, first: None, last: None},
            Holiday::MovableYearlyDay{month: 11, day: 2, first: None, last: None},

            Holiday::MovableYearlyDay{month: 11, day: 10, first: None, last: Some(2019)},
            Holiday::MovableYearlyDay{month: 11, day: 17, first: Some(2019), last: None},
            Holiday::MovableYearlyDay{month: 11, day: 24, first: Some(2019), last: Some(2019)},
        ];
        let cal = Calendar::calc_calendar(&holidays, 2018, 2020);
        assert_eq!(true, cal.is_holiday(NaiveDate::from_ymd(2018, 11, 1)));
        assert_eq!(true, cal.is_holiday(NaiveDate::from_ymd(2018, 11, 2)));
        assert_eq!(true, cal.is_holiday(NaiveDate::from_ymd(2019, 11, 1)));
        assert_eq!(true, cal.is_holiday(NaiveDate::from_ymd(2019, 11, 4)));
        assert_eq!(true, cal.is_holiday(NaiveDate::from_ymd(2020, 11, 2)));
        assert_eq!(true, cal.is_holiday(NaiveDate::from_ymd(2020, 11, 3)));

        assert_eq!(true, cal.is_holiday(NaiveDate::from_ymd(2018, 11, 12)));
        assert_eq!(true, cal.is_holiday(NaiveDate::from_ymd(2019, 11, 11)));
        assert_eq!(false, cal.is_holiday(NaiveDate::from_ymd(2020, 11, 10)));
        assert_eq!(false, cal.is_holiday(NaiveDate::from_ymd(2018, 11, 19)));
        assert_eq!(true, cal.is_holiday(NaiveDate::from_ymd(2019, 11, 18)));
        assert_eq!(true, cal.is_holiday(NaiveDate::from_ymd(2020, 11, 17)));
        assert_eq!(false, cal.is_holiday(NaiveDate::from_ymd(2018, 11, 26)));
        assert_eq!(true, cal.is_holiday(NaiveDate::from_ymd(2019, 11, 25)));
        assert_eq!(false, cal.is_holiday(NaiveDate::from_ymd(2020, 11, 24)));
    }

    #[test]
    // Good Friday example
    fn test_easter_offset() {        
        let holidays = vec![
            Holiday::EasterOffset(-2),
        ];
        let cal = Calendar::calc_calendar(&holidays, 2019, 2020);
        assert_eq!(false, cal.is_business_day(NaiveDate::from_ymd(2019, 4, 19)));
        assert_eq!(false, cal.is_business_day(NaiveDate::from_ymd(2020, 4, 10)));
    }

    #[test]
    fn test_month_weekday() {        
        let holidays = vec![
            Holiday::MonthWeekday{month: 11, weekday: Weekday::Mon, nth: NthWeekday::First, first: None, last: None },
            Holiday::MonthWeekday{month: 11, weekday: Weekday::Tue, nth: NthWeekday::Second, first: None, last: None },
            Holiday::MonthWeekday{month: 11, weekday: Weekday::Wed, nth: NthWeekday::Third, first: None, last: None },
            Holiday::MonthWeekday{month: 11, weekday: Weekday::Thu, nth: NthWeekday::Fourth, first: None, last: None },
            Holiday::MonthWeekday{month: 11, weekday: Weekday::Fri, nth: NthWeekday::Last, first: None, last: None },

            Holiday::MonthWeekday{month: 11, weekday: Weekday::Sat, nth: NthWeekday::First, first: None, last: Some(2018) },
            Holiday::MonthWeekday{month: 11, weekday: Weekday::Sun, nth: NthWeekday::Last, first: Some(2020), last: None },
        ];
        let cal = Calendar::calc_calendar(&holidays, 2018, 2020);
        assert_eq!(true, cal.is_holiday(NaiveDate::from_ymd(2019, 11, 4)));
        assert_eq!(true, cal.is_holiday(NaiveDate::from_ymd(2019, 11, 12)));
        assert_eq!(true, cal.is_holiday(NaiveDate::from_ymd(2019, 11, 20)));
        assert_eq!(true, cal.is_holiday(NaiveDate::from_ymd(2019, 11, 28)));
        assert_eq!(true, cal.is_holiday(NaiveDate::from_ymd(2019, 11, 29)));

        assert_eq!(true, cal.is_holiday(NaiveDate::from_ymd(2018, 11, 3)));
        assert_eq!(false, cal.is_holiday(NaiveDate::from_ymd(2019, 11, 2)));
        assert_eq!(false, cal.is_holiday(NaiveDate::from_ymd(2020, 11, 7)));
        assert_eq!(false, cal.is_holiday(NaiveDate::from_ymd(2018, 11, 25)));
        assert_eq!(false, cal.is_holiday(NaiveDate::from_ymd(2019, 11, 24)));
        assert_eq!(true, cal.is_holiday(NaiveDate::from_ymd(2020, 11, 29)));
    }
}