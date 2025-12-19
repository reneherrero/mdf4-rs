use crate::Result;
use crate::blocks::conversion::base::ConversionBlock;
use crate::types::DecodedValue;
use alloc::string::String;
use alloc::vec::Vec;

/// Attempts to extract a numeric value from a [`DecodedValue`].
/// Returns `Some(f64)` if the input is numeric, or `None` otherwise.
pub fn extract_numeric(value: &DecodedValue) -> Option<f64> {
    match value {
        DecodedValue::Float(n) => Some(*n),
        DecodedValue::UnsignedInteger(n) => Some(*n as f64),
        DecodedValue::SignedInteger(n) => Some(*n as f64),
        _ => None,
    }
}

/// Apply a linear conversion.
pub fn apply_linear(block: &ConversionBlock, value: DecodedValue) -> Result<DecodedValue> {
    if let Some(raw) = extract_numeric(&value) {
        if block.cc_val.len() >= 2 {
            let result = block.cc_val[0] + block.cc_val[1] * raw;
            Ok(DecodedValue::Float(result))
        } else {
            Ok(DecodedValue::Float(raw))
        }
    } else {
        Ok(value)
    }
}

/// Apply a rational conversion.
pub fn apply_rational(block: &ConversionBlock, value: DecodedValue) -> Result<DecodedValue> {
    if let Some(raw) = extract_numeric(&value) {
        if block.cc_val.len() >= 6 {
            let p1 = block.cc_val[0];
            let p2 = block.cc_val[1];
            let p3 = block.cc_val[2];
            let p4 = block.cc_val[3];
            let p5 = block.cc_val[4];
            let p6 = block.cc_val[5];

            let num = p1 * raw * raw + p2 * raw + p3;
            let den = p4 * raw * raw + p5 * raw + p6;
            if den.abs() > f64::EPSILON {
                Ok(DecodedValue::Float(num / den))
            } else {
                Ok(DecodedValue::Float(raw))
            }
        } else {
            Ok(DecodedValue::Float(raw))
        }
    } else {
        Ok(value)
    }
}

/// Apply an algebraic conversion using a stored formula.
pub fn apply_algebraic(block: &ConversionBlock, value: DecodedValue) -> Result<DecodedValue> {
    if let (Some(raw), Some(expr_str)) = (extract_numeric(&value), block.formula.as_ref()) {
        match eval_formula(expr_str, raw) {
            Ok(res) => Ok(DecodedValue::Float(res)),
            Err(_) => Ok(DecodedValue::Float(raw)),
        }
    } else {
        Ok(value)
    }
}

/// Simple expression evaluator for MCD-2 MC algebraic formulas.
/// Supports: +, -, *, /, ^, parentheses, and the variable X.
fn eval_formula(expr: &str, x: f64) -> core::result::Result<f64, &'static str> {
    let tokens = tokenize(expr)?;
    let mut pos = 0;
    parse_expr(&tokens, &mut pos, x)
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(f64),
    Variable, // X
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    LParen,
    RParen,
}

fn tokenize(expr: &str) -> core::result::Result<Vec<Token>, &'static str> {
    let mut tokens = Vec::new();
    let mut chars = expr.chars().peekable();

    while let Some(&c) = chars.peek() {
        match c {
            ' ' | '\t' | '\n' | '\r' => {
                chars.next();
            }
            '+' => {
                tokens.push(Token::Plus);
                chars.next();
            }
            '-' => {
                tokens.push(Token::Minus);
                chars.next();
            }
            '*' => {
                chars.next();
                if chars.peek() == Some(&'*') {
                    chars.next();
                    tokens.push(Token::Caret); // ** as power
                } else {
                    tokens.push(Token::Star);
                }
            }
            '/' => {
                tokens.push(Token::Slash);
                chars.next();
            }
            '^' => {
                tokens.push(Token::Caret);
                chars.next();
            }
            '(' => {
                tokens.push(Token::LParen);
                chars.next();
            }
            ')' => {
                tokens.push(Token::RParen);
                chars.next();
            }
            'X' | 'x' => {
                tokens.push(Token::Variable);
                chars.next();
            }
            '0'..='9' | '.' => {
                let mut num_str = String::new();
                while let Some(&ch) = chars.peek() {
                    if ch.is_ascii_digit() || ch == '.' || ch == 'e' || ch == 'E' {
                        num_str.push(ch);
                        chars.next();
                        // Handle exponent sign
                        if (ch == 'e' || ch == 'E') && chars.peek() == Some(&'-') {
                            num_str.push('-');
                            chars.next();
                        } else if (ch == 'e' || ch == 'E') && chars.peek() == Some(&'+') {
                            num_str.push('+');
                            chars.next();
                        }
                    } else {
                        break;
                    }
                }
                let n: f64 = num_str.parse().map_err(|_| "Invalid number")?;
                tokens.push(Token::Number(n));
            }
            _ => return Err("Unexpected character"),
        }
    }

    Ok(tokens)
}

// Grammar:
// expr   = term (('+' | '-') term)*
// term   = power (('*' | '/') power)*
// power  = unary ('^' power)?
// unary  = '-' unary | primary
// primary = NUMBER | VARIABLE | '(' expr ')'

fn parse_expr(
    tokens: &[Token],
    pos: &mut usize,
    x: f64,
) -> core::result::Result<f64, &'static str> {
    let mut left = parse_term(tokens, pos, x)?;

    while *pos < tokens.len() {
        match &tokens[*pos] {
            Token::Plus => {
                *pos += 1;
                left += parse_term(tokens, pos, x)?;
            }
            Token::Minus => {
                *pos += 1;
                left -= parse_term(tokens, pos, x)?;
            }
            _ => break,
        }
    }

    Ok(left)
}

fn parse_term(
    tokens: &[Token],
    pos: &mut usize,
    x: f64,
) -> core::result::Result<f64, &'static str> {
    let mut left = parse_power(tokens, pos, x)?;

    while *pos < tokens.len() {
        match &tokens[*pos] {
            Token::Star => {
                *pos += 1;
                left *= parse_power(tokens, pos, x)?;
            }
            Token::Slash => {
                *pos += 1;
                let right = parse_power(tokens, pos, x)?;
                if right.abs() < f64::EPSILON {
                    return Err("Division by zero");
                }
                left /= right;
            }
            _ => break,
        }
    }

    Ok(left)
}

fn parse_power(
    tokens: &[Token],
    pos: &mut usize,
    x: f64,
) -> core::result::Result<f64, &'static str> {
    let base = parse_unary(tokens, pos, x)?;

    if *pos < tokens.len() && tokens[*pos] == Token::Caret {
        *pos += 1;
        let exp = parse_power(tokens, pos, x)?; // Right associative
        Ok(pow_compat(base, exp))
    } else {
        Ok(base)
    }
}

/// Round to nearest integer (works without std).
#[inline]
fn round_compat(x: f64) -> f64 {
    if x >= 0.0 {
        (x + 0.5) as i64 as f64
    } else {
        (x - 0.5) as i64 as f64
    }
}

/// Integer power function (works without std).
/// Uses exponentiation by squaring for efficiency.
#[inline]
fn powi_compat(base: f64, exp: i32) -> f64 {
    if exp == 0 {
        return 1.0;
    }
    let mut result = 1.0;
    let mut b = base;
    let mut n = exp.unsigned_abs();
    while n > 0 {
        if n & 1 != 0 {
            result *= b;
        }
        b *= b;
        n >>= 1;
    }
    if exp < 0 { 1.0 / result } else { result }
}

/// Power function that works in both std and no_std environments.
/// Uses integer exponentiation for integer exponents, and `powf` for
/// non-integer exponents (requires std).
#[inline]
fn pow_compat(base: f64, exp: f64) -> f64 {
    // Check if exponent is close to an integer
    let exp_rounded = round_compat(exp);
    if (exp - exp_rounded).abs() < 1e-10 {
        // Use integer power for integer exponents
        let exp_int = exp_rounded as i32;
        powi_compat(base, exp_int)
    } else {
        // Non-integer exponent requires powf
        #[cfg(feature = "std")]
        {
            base.powf(exp)
        }
        #[cfg(not(feature = "std"))]
        {
            // For no_std without libm, we return NaN for non-integer exponents
            // This is a rare edge case in MDF4 algebraic formulas
            core::f64::NAN
        }
    }
}

fn parse_unary(
    tokens: &[Token],
    pos: &mut usize,
    x: f64,
) -> core::result::Result<f64, &'static str> {
    if *pos < tokens.len() && tokens[*pos] == Token::Minus {
        *pos += 1;
        Ok(-parse_unary(tokens, pos, x)?)
    } else {
        parse_primary(tokens, pos, x)
    }
}

fn parse_primary(
    tokens: &[Token],
    pos: &mut usize,
    x: f64,
) -> core::result::Result<f64, &'static str> {
    if *pos >= tokens.len() {
        return Err("Unexpected end of expression");
    }

    match &tokens[*pos] {
        Token::Number(n) => {
            *pos += 1;
            Ok(*n)
        }
        Token::Variable => {
            *pos += 1;
            Ok(x)
        }
        Token::LParen => {
            *pos += 1;
            let result = parse_expr(tokens, pos, x)?;
            if *pos >= tokens.len() || tokens[*pos] != Token::RParen {
                return Err("Expected closing parenthesis");
            }
            *pos += 1;
            Ok(result)
        }
        _ => Err("Unexpected token"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_x() {
        assert!((eval_formula("X", 5.0).unwrap() - 5.0).abs() < 1e-10);
        assert!((eval_formula("x", 5.0).unwrap() - 5.0).abs() < 1e-10);
    }

    #[test]
    fn test_linear() {
        assert!((eval_formula("2*X + 1", 3.0).unwrap() - 7.0).abs() < 1e-10);
        assert!((eval_formula("X * 2 + 1", 3.0).unwrap() - 7.0).abs() < 1e-10);
    }

    #[test]
    fn test_power() {
        assert!((eval_formula("X^2", 3.0).unwrap() - 9.0).abs() < 1e-10);
        assert!((eval_formula("X**2", 3.0).unwrap() - 9.0).abs() < 1e-10);
    }

    #[test]
    fn test_parentheses() {
        assert!((eval_formula("(X + 1) * 2", 3.0).unwrap() - 8.0).abs() < 1e-10);
    }

    #[test]
    fn test_negative() {
        assert!((eval_formula("-X", 5.0).unwrap() - (-5.0)).abs() < 1e-10);
        assert!((eval_formula("X - 3", 5.0).unwrap() - 2.0).abs() < 1e-10);
    }

    #[test]
    fn test_division() {
        assert!((eval_formula("X / 2", 6.0).unwrap() - 3.0).abs() < 1e-10);
    }

    #[test]
    fn test_complex() {
        // X^2 + 3*X + 1 with X=2 should be 4 + 6 + 1 = 11
        assert!((eval_formula("X^2 + 3*X + 1", 2.0).unwrap() - 11.0).abs() < 1e-10);
    }

    #[test]
    fn test_scientific_notation() {
        assert!((eval_formula("1e3 * X", 2.0).unwrap() - 2000.0).abs() < 1e-10);
        assert!((eval_formula("1.5e-2 * X", 100.0).unwrap() - 1.5).abs() < 1e-10);
    }
}
