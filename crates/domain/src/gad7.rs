use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Gad7Band {
    Minimal,
    Mild,
    Moderate,
    Severe,
}

pub fn score(answers: &[u8]) -> Result<(i16, Gad7Band), &'static str> {
    if answers.len() != 7 || answers.iter().any(|value| *value > 3) {
        return Err("Ответьте на все семь вопросов, выбрав значение от 0 до 3");
    }
    let total: i16 = answers.iter().map(|value| i16::from(*value)).sum();
    let band = match total {
        0..=4 => Gad7Band::Minimal,
        5..=9 => Gad7Band::Mild,
        10..=14 => Gad7Band::Moderate,
        _ => Gad7Band::Severe,
    };
    Ok((total, band))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checks_published_cutoffs_and_complete_answers() {
        for (total, band) in [
            (0, Gad7Band::Minimal),
            (4, Gad7Band::Minimal),
            (5, Gad7Band::Mild),
            (9, Gad7Band::Mild),
            (10, Gad7Band::Moderate),
            (14, Gad7Band::Moderate),
            (15, Gad7Band::Severe),
            (21, Gad7Band::Severe),
        ] {
            let mut left = total;
            let answers: Vec<u8> = (0..7)
                .map(|_| {
                    let value = left.min(3);
                    left -= value;
                    value
                })
                .collect();
            assert_eq!(score(&answers), Ok((i16::from(total), band)));
        }
        assert!(score(&[]).is_err());
        assert!(score(&[0; 6]).is_err());
        assert!(score(&[0; 8]).is_err());
        assert!(score(&[0, 0, 0, 0, 0, 0, 4]).is_err());
    }
}
