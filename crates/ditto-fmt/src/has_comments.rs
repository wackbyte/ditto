use ditto_cst::*;

pub trait HasComments {
    fn has_comments(&self) -> bool;
    fn has_leading_comments(&self) -> bool;
}

impl<T: HasComments> HasComments for Box<T> {
    fn has_comments(&self) -> bool {
        self.as_ref().has_comments()
    }
    fn has_leading_comments(&self) -> bool {
        self.as_ref().has_leading_comments()
    }
}

impl HasComments for Expression {
    fn has_comments(&self) -> bool {
        match self {
            Self::True(keyword) => keyword.0.has_comments(),
            Self::False(keyword) => keyword.0.has_comments(),
            Self::Unit(keyword) => keyword.0.has_comments(),
            Self::String(token) => token.has_comments(),
            Self::Int(token) => token.has_comments(),
            Self::Float(token) => token.has_comments(),
            Self::Constructor(constructor) => constructor.has_comments(),
            Self::Variable(variable) => variable.has_comments(),
            Self::Parens(parens) => parens.has_comments(),
            Self::Array(brackets) => brackets.has_comments(),
            Self::If {
                if_keyword,
                condition,
                then_keyword,
                true_clause,
                else_keyword,
                false_clause,
            } => {
                if_keyword.0.has_comments()
                    || condition.has_comments()
                    || then_keyword.0.has_comments()
                    || true_clause.has_comments()
                    || else_keyword.0.has_comments()
                    || false_clause.has_comments()
            }
            Self::Function {
                parameters,
                return_type_annotation,
                right_arrow,
                body,
            } => {
                parameters.has_comments()
                    || return_type_annotation.has_comments()
                    || right_arrow.0.has_comments()
                    || body.has_comments()
            }
            Self::Call {
                function,
                arguments,
            } => function.has_comments() || arguments.has_comments(),
        }
    }

    fn has_leading_comments(&self) -> bool {
        match self {
            Self::True(keyword) => keyword.0.has_leading_comments(),
            Self::False(keyword) => keyword.0.has_leading_comments(),
            Self::Unit(keyword) => keyword.0.has_leading_comments(),
            Self::String(token) => token.has_leading_comments(),
            Self::Int(token) => token.has_leading_comments(),
            Self::Float(token) => token.has_leading_comments(),
            Self::Constructor(constructor) => constructor.has_leading_comments(),
            Self::Variable(variable) => variable.has_leading_comments(),
            Self::Parens(parens) => parens.open_paren.0.has_leading_comments(),
            Self::Array(brackets) => brackets.open_bracket.0.has_leading_comments(),
            Self::If { if_keyword, .. } => if_keyword.0.has_leading_comments(),
            Self::Function { box parameters, .. } => parameters.open_paren.0.has_leading_comments(),
            Self::Call { function, .. } => function.has_leading_comments(),
        }
    }
}

impl HasComments for Type {
    fn has_comments(&self) -> bool {
        match self {
            Self::Parens(parens) => parens.has_comments(),
            Self::Variable(variable) => variable.has_comments(),
            Self::Constructor(constructor) => constructor.has_comments(),
            Self::Function {
                parameters,
                right_arrow,
                return_type,
            } => {
                parameters.has_comments()
                    || right_arrow.0.has_comments()
                    || return_type.has_comments()
            }
            Self::Call {
                function,
                arguments,
            } => function.has_comments() || arguments.has_comments(),
        }
    }
    fn has_leading_comments(&self) -> bool {
        match self {
            Self::Parens(parens) => parens.open_paren.0.has_leading_comments(),
            Self::Variable(variable) => variable.has_leading_comments(),
            Self::Constructor(constructor) => constructor.has_leading_comments(),
            Self::Function { parameters, .. } => parameters.open_paren.0.has_leading_comments(),
            Self::Call { function, .. } => function.has_leading_comments(),
        }
    }
}

impl HasComments for TypeCallFunction {
    fn has_comments(&self) -> bool {
        match self {
            Self::Constructor(constructor) => constructor.has_comments(),
            Self::Variable(variable) => variable.has_comments(),
        }
    }
    fn has_leading_comments(&self) -> bool {
        match self {
            Self::Constructor(constructor) => constructor.has_leading_comments(),
            Self::Variable(variable) => variable.has_leading_comments(),
        }
    }
}

impl HasComments for TypeAnnotation {
    fn has_comments(&self) -> bool {
        self.0 .0.has_comments() || self.1.has_comments()
    }
    fn has_leading_comments(&self) -> bool {
        self.0 .0.has_leading_comments()
    }
}

impl<T: HasComments> HasComments for Parens<T> {
    fn has_comments(&self) -> bool {
        self.open_paren.0.has_comments()
            || self.value.has_comments()
            || self.close_paren.0.has_comments()
    }
    fn has_leading_comments(&self) -> bool {
        self.open_paren.0.has_leading_comments()
    }
}

impl<T: HasComments> HasComments for Brackets<T> {
    fn has_comments(&self) -> bool {
        self.open_bracket.0.has_comments()
            || self.value.has_comments()
            || self.close_bracket.0.has_comments()
    }
    fn has_leading_comments(&self) -> bool {
        self.open_bracket.0.has_leading_comments()
    }
}

impl<T: HasComments> HasComments for CommaSep1<T> {
    fn has_comments(&self) -> bool {
        self.head.has_comments()
            || self.tail.has_comments()
            || self
                .trailing_comma
                .as_ref()
                .map_or(false, |trailing_comma| trailing_comma.has_comments())
    }
    fn has_leading_comments(&self) -> bool {
        self.head.has_leading_comments()
    }
}

impl<T: HasComments> HasComments for Vec<T> {
    fn has_comments(&self) -> bool {
        self.iter().any(|x| x.has_comments())
    }
    fn has_leading_comments(&self) -> bool {
        if let Some(first) = self.first() {
            first.has_leading_comments()
        } else {
            false
        }
    }
}

impl<Value: HasComments> HasComments for Qualified<Value> {
    fn has_comments(&self) -> bool {
        self.module_name
            .as_ref()
            .map_or(false, |module_name| module_name.has_comments())
            || self.value.has_comments()
    }
    fn has_leading_comments(&self) -> bool {
        if let Some(module_name) = self.module_name.as_ref() {
            module_name.has_leading_comments()
        } else {
            self.value.has_leading_comments()
        }
    }
}

impl<T: HasComments> HasComments for Option<T> {
    fn has_comments(&self) -> bool {
        self.as_ref().map_or(false, |x| x.has_comments())
    }
    fn has_leading_comments(&self) -> bool {
        self.as_ref().map_or(false, |x| x.has_leading_comments())
    }
}

impl<Fst: HasComments, Snd: HasComments> HasComments for (Fst, Snd) {
    fn has_comments(&self) -> bool {
        self.0.has_comments() || self.1.has_comments()
    }
    fn has_leading_comments(&self) -> bool {
        self.0.has_leading_comments()
    }
}

impl HasComments for Import {
    fn has_comments(&self) -> bool {
        match self {
            Self::Value(name) => name.has_comments(),
            Self::Type(proper_name, everything) => {
                proper_name.has_comments() || everything.has_comments()
            }
        }
    }
    fn has_leading_comments(&self) -> bool {
        match self {
            Self::Value(name) => name.has_leading_comments(),
            Self::Type(proper_name, _everything) => proper_name.has_leading_comments(),
        }
    }
}

impl HasComments for Export {
    fn has_comments(&self) -> bool {
        match self {
            Self::Value(name) => name.has_comments(),
            Self::Type(proper_name, everything) => {
                proper_name.has_comments() || everything.has_comments()
            }
        }
    }
    fn has_leading_comments(&self) -> bool {
        match self {
            Self::Value(name) => name.has_leading_comments(),
            Self::Type(proper_name, _everything) => proper_name.has_leading_comments(),
        }
    }
}

impl HasComments for Dot {
    fn has_comments(&self) -> bool {
        self.0.has_comments()
    }
    fn has_leading_comments(&self) -> bool {
        self.0.has_leading_comments()
    }
}

impl HasComments for DoubleDot {
    fn has_comments(&self) -> bool {
        self.0.has_comments()
    }
    fn has_leading_comments(&self) -> bool {
        self.0.has_leading_comments()
    }
}

impl HasComments for Comma {
    fn has_comments(&self) -> bool {
        self.0.has_comments()
    }
    fn has_leading_comments(&self) -> bool {
        self.0.has_leading_comments()
    }
}

impl HasComments for Name {
    fn has_comments(&self) -> bool {
        self.0.has_comments()
    }
    fn has_leading_comments(&self) -> bool {
        self.0.has_leading_comments()
    }
}

impl HasComments for ProperName {
    fn has_comments(&self) -> bool {
        self.0.has_comments()
    }
    fn has_leading_comments(&self) -> bool {
        self.0.has_leading_comments()
    }
}
