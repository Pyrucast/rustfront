//! Expressions of the time `t`, as given to the `<function>` evolutions:
//! `'1.e-3*t'`, `'s0*sin(t/900.)'`.
//!
//! Numbers, `t`, the constants declared by `@Real`, the operators `+ - * /`
//! and `^` (or `**`) for the power, parentheses, and the functions of
//! [`FUNCTIONS`]. The constants are replaced by their values when parsing.

/// A function of one variable.
type Function = fn(f64) -> f64;

/// The functions an expression may call.
const FUNCTIONS: [(&str, Function); 15] = [
    ("sin", f64::sin),
    ("cos", f64::cos),
    ("tan", f64::tan),
    ("asin", f64::asin),
    ("acos", f64::acos),
    ("atan", f64::atan),
    ("sinh", f64::sinh),
    ("cosh", f64::cosh),
    ("tanh", f64::tanh),
    ("exp", f64::exp),
    ("log", f64::ln),
    ("ln", f64::ln),
    ("log10", f64::log10),
    ("sqrt", f64::sqrt),
    ("abs", f64::abs),
];

/// A binary operator.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Operator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Power,
}

/// An expression of the time, ready to be evaluated.
#[derive(Clone, Debug, PartialEq)]
pub enum Expression {
    Number(f64),
    Time,
    Negate(Box<Expression>),
    Binary(Operator, Box<Expression>, Box<Expression>),
    /// A call to the function of index `0` in [`FUNCTIONS`]: an index rather
    /// than a function pointer, which cannot be compared reliably.
    Call(usize, Box<Expression>),
}

#[derive(Clone, Debug, PartialEq)]
enum Token {
    Number(f64),
    Name(String),
    Symbol(char),
}

/// The tokens of an expression and the position of the next one to read.
struct Parser<'a> {
    tokens: Vec<Token>,
    position: usize,
    constants: &'a [(String, f64)],
}

impl Parser<'_> {
    /// Parses an operand followed by the binary operators whose precedence is
    /// at least `minimum`: precedence climbing, `^` being right associative
    /// and binding tighter than the unary minus (`-t^2` is `-(t^2)`).
    fn expression(&mut self, minimum: u8) -> Result<Expression, String> {
        let token = self.tokens.get(self.position).cloned();
        self.position += 1;
        let mut left = match token {
            Some(Token::Number(value)) => Expression::Number(value),
            Some(Token::Symbol('-')) => Expression::Negate(Box::new(self.expression(3)?)),
            Some(Token::Symbol('+')) => self.expression(3)?,
            Some(Token::Symbol('(')) => {
                let inner = self.expression(0)?;
                if self.tokens.get(self.position) != Some(&Token::Symbol(')')) {
                    return Err("missing `)`".to_owned());
                }
                self.position += 1;
                inner
            }
            Some(Token::Name(name))
                if self.tokens.get(self.position) == Some(&Token::Symbol('(')) =>
            {
                let function = FUNCTIONS
                    .iter()
                    .position(|(n, _)| *n == name)
                    .ok_or_else(|| format!("unknown function `{name}`"))?;
                // The parenthesised argument alone, above every precedence:
                // `sin(t)^2` is `(sin t)^2`.
                Expression::Call(function, Box::new(self.expression(5)?))
            }
            Some(Token::Name(name)) if name == "t" => Expression::Time,
            Some(Token::Name(name)) => {
                let value = self
                    .constants
                    .iter()
                    .find(|(n, _)| *n == name)
                    .ok_or_else(|| format!("unknown variable `{name}`"))?
                    .1;
                Expression::Number(value)
            }
            Some(Token::Symbol(c)) => return Err(format!("unexpected `{c}`")),
            None => return Err("unexpected end of expression".to_owned()),
        };
        while let Some(Token::Symbol(c)) = self.tokens.get(self.position) {
            let (operator, precedence) = match c {
                '+' => (Operator::Add, 1),
                '-' => (Operator::Subtract, 1),
                '*' => (Operator::Multiply, 2),
                '/' => (Operator::Divide, 2),
                '^' => (Operator::Power, 4),
                _ => break,
            };
            if precedence < minimum {
                break;
            }
            self.position += 1;
            // Left associative operators parse their right operand one level
            // higher; the power, right associative, at its own level.
            let next = if operator == Operator::Power {
                precedence
            } else {
                precedence + 1
            };
            let right = self.expression(next)?;
            left = Expression::Binary(operator, Box::new(left), Box::new(right));
        }
        Ok(left)
    }
}

impl Expression {
    /// Parses `text`, replacing the names of `constants` by their values.
    pub fn parse(text: &str, constants: &[(String, f64)]) -> Result<Expression, String> {
        let chars: Vec<char> = text.chars().collect();
        let mut tokens = Vec::new();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            if c.is_whitespace() {
                i += 1;
            } else if c.is_ascii_digit() || c == '.' {
                // Digits and dots, then an exponent: `1.e-3`, `2E5`.
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    i += 1;
                }
                if i < chars.len() && (chars[i] == 'e' || chars[i] == 'E') {
                    let sign =
                        usize::from(chars.get(i + 1).is_some_and(|s| *s == '+' || *s == '-'));
                    if chars.get(i + 1 + sign).is_some_and(char::is_ascii_digit) {
                        i += 1 + sign;
                        while i < chars.len() && chars[i].is_ascii_digit() {
                            i += 1;
                        }
                    }
                }
                let number: String = chars[start..i].iter().collect();
                let value = number
                    .parse()
                    .map_err(|_| format!("`{number}` is not a number"))?;
                tokens.push(Token::Number(value));
            } else if c.is_alphabetic() || c == '_' {
                let start = i;
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                tokens.push(Token::Name(chars[start..i].iter().collect()));
            } else if c == '*' && chars.get(i + 1) == Some(&'*') {
                tokens.push(Token::Symbol('^'));
                i += 2;
            } else if "+-*/^()".contains(c) {
                tokens.push(Token::Symbol(c));
                i += 1;
            } else {
                return Err(format!("unexpected `{c}` in `{text}`"));
            }
        }
        let mut parser = Parser {
            tokens,
            position: 0,
            constants,
        };
        let expression = parser
            .expression(0)
            .map_err(|e| format!("{e} in `{text}`"))?;
        if parser.position != parser.tokens.len() {
            return Err(format!("unexpected trailing text in `{text}`"));
        }
        Ok(expression)
    }

    /// The value of the expression at the time `t`.
    pub fn value(&self, t: f64) -> f64 {
        match self {
            Expression::Number(value) => *value,
            Expression::Time => t,
            Expression::Negate(e) => -e.value(t),
            Expression::Binary(operator, a, b) => {
                let (a, b) = (a.value(t), b.value(t));
                match operator {
                    Operator::Add => a + b,
                    Operator::Subtract => a - b,
                    Operator::Multiply => a * b,
                    Operator::Divide => a / b,
                    Operator::Power => a.powf(b),
                }
            }
            Expression::Call(function, e) => FUNCTIONS[*function].1(e.value(t)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_follow_the_usual_precedences() {
        let constants = [("s0".to_owned(), 2.0)];
        for (text, t, expected) in [
            ("1.e-3*t", 2.0, 2e-3),
            ("1+2*3", 0.0, 7.0),
            ("(1+2)*3", 0.0, 9.0),
            ("2^3^2", 0.0, 512.0),
            ("2**3", 0.0, 8.0),
            ("-t^2", 3.0, -9.0),
            ("10-4-3", 0.0, 3.0),
            ("8/4/2", 0.0, 1.0),
            ("2*-t", 1.5, -3.0),
            ("s0*sin(t/2)", core::f64::consts::PI, 2.0),
            ("exp(log(t))", 5.0, 5.0),
            ("sin(t)^2+cos(t)^2", 0.7, 1.0),
            ("sqrt(t)*2E2", 4.0, 400.0),
        ] {
            let e = Expression::parse(text, &constants).unwrap();
            let found = e.value(t);
            assert!(
                (found - expected).abs() < 1e-12 * expected.abs().max(1.0),
                "{text} at {t}: {found}, expected {expected}"
            );
        }
    }

    #[test]
    fn mistakes_are_reported() {
        for (text, message) in [
            ("1+", "unexpected end"),
            ("foo(t)", "unknown function `foo`"),
            ("2*x", "unknown variable `x`"),
            ("(1+2", "missing `)`"),
            ("1 2", "trailing"),
            ("1;", "unexpected `;`"),
        ] {
            let error = Expression::parse(text, &[]).unwrap_err();
            assert!(error.contains(message), "{text}: {error}");
        }
    }
}
