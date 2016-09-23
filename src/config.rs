use std::str::Chars;

use std::collections::BTreeMap;
use std::collections::btree_map::Entry::{Occupied, Vacant};

use std::str::FromStr;
use std::slice::Iter;

pub enum ParameterValue {
    String{line:usize, value:String},
    List(List),
    Map(Map),
}

impl ParameterValue {
    pub fn getString<'a>( &'a self ) -> Result<&'a String,String>{
        match *self {
            ParameterValue::String{ line, ref value } =>
                Ok(value),
            ParameterValue::Map( ref map ) =>
                Err(format!("Line {} : Parameter is not string", map.line)),
            ParameterValue::List( ref list ) =>
                Err(format!("Line {} : Parameter is not string", list.line)),
        }
    }

    pub fn getMap<'a>( &'a self ) -> Result<&'a Map,String>{
        match *self {
            ParameterValue::String{ line, ref value } =>
                Err(format!("Line {} : Parameter is not map", line)),
            ParameterValue::Map( ref map ) =>
                Ok( map ),
            ParameterValue::List( ref list ) =>
                Err(format!("Line {} : Parameter is not map", list.line)),
        }
    }

    pub fn getList<'a>( &'a self ) -> Result<&'a List,String>{
        match *self {
            ParameterValue::String{ line, ref value } =>
                Err(format!("Line {} : Parameter is not list", line)),
            ParameterValue::Map( ref map ) =>
                Err(format!("Line {} : Parameter is not list", map.line)),
            ParameterValue::List( ref list ) =>
                Ok( list ),
        }
    }
}

pub struct Map{
    line:usize,
    params:BTreeMap<String, ParameterValue>,
}

impl Map {
    fn parse( cur:&mut TextCursor, endsWith:char, mapLine:usize ) -> Result<Map, String>{
        let mut params=BTreeMap::new();

        loop {
            match try!(Lexeme::next( cur )) {
                Lexeme::String( paramName ) => {
                    let line=cur.line;

                    if try!(Lexeme::next( cur )) != Lexeme::Set {
                        return Err( format!("Line {} : Expected : or =", line ));
                    }

                    let paramValue=match try!(Lexeme::next( cur )) {
                        Lexeme::Bracket( '{') =>
                            ParameterValue::Map( try!(Map::parse( cur, '}', line )) ),
                        Lexeme::Bracket( '[') =>
                            ParameterValue::List( try!(List::parse( cur, line )) ),
                        Lexeme::String( s ) =>
                            ParameterValue::String{line:line, value:s },
                        Lexeme::NewLine | Lexeme::Comma | Lexeme::EOF | Lexeme::Bracket('}') =>
                            return Err(format!("Line {} : Value of parameter has been missed",line)),
                        _=>
                            return Err(format!("Line {} : Expected string(\"..\"), list([...]), map(_.._)",line)),
                    };

                    match params.entry( paramName ) {
                        Vacant( entry ) => {entry.insert( paramValue );},
                        Occupied( e ) => return Err(format!("Line {} : Parameter has been declarated before", line)),
                    }

                    match try!(Lexeme::next( cur )){
                        Lexeme::EOF => {
                            if endsWith!='\0' {
                                return Err(format!("Line {} : Expected _, but EOF was found",line));
                            }

                            break;
                        },
                        Lexeme::Bracket('}') => {
                            if endsWith!='}' {
                                return Err(format!("Line {} : Expected EOF, but _ was found",line));
                            }

                            break;
                        },
                        Lexeme::NewLine | Lexeme::Comma => {},
                        _=>return Err(format!("Line {} : Expected new line or , or {}", line, endsWith)),
                    }
                },
                Lexeme::NewLine => {},
                Lexeme::EOF => break,
                _=>return Err(format!("Line {} : Expected parameter name",cur.line)),
            }
        }

        Ok(
            Map{
                line:mapLine,
                params:params,
            }
        )
    }

    pub fn getMap<'a>( &'a self, name:&'a str ) -> Result<&'a Map,String>{
        match self.params.get( name ){
            Some( pv ) => {
                match *pv {
                    ParameterValue::String{ line, ref value } =>
                        Err(format!("Line {} : Parameter \"{}\" is not map", line, name)),
                    ParameterValue::Map( ref map ) =>
                        Ok( map ),
                    ParameterValue::List( ref list ) =>
                        Err(format!("Line {} : Parameter \"{}\" is not map", list.line, name)),
                }
            },
            None=>Err(format!("Line {} : Map has no parameter \"{}\"", self.line,name)),
        }
    }

    pub fn getList<'a>( &'a self, name:&'a str ) -> Result<&'a List,String>{
        match self.params.get( name ){
            Some( pv ) => {
                match *pv {
                    ParameterValue::String{ line, ref value } =>
                        Err(format!("Line {} : Parameter \"{}\" is not list", line, name)),
                    ParameterValue::Map( ref map ) =>
                        Err(format!("Line {} : Parameter \"{}\" is not list", map.line, name)),
                    ParameterValue::List( ref list ) =>
                        Ok( list ),
                }
            },
            None=>Err(format!("Line {} : Map has no parameter \"{}\"", self.line,name)),
        }
    }

    pub fn getString<'a>( &'a self, name:&'a str ) -> Result<&'a String,String>{
        match self.params.get( name ){
            Some( pv ) => {
                match *pv {
                    ParameterValue::String{ line, ref value } =>
                        Ok(value),
                    ParameterValue::Map( ref map ) =>
                        Err(format!("Line {} : Parameter \"{}\" is not string", map.line, name)),
                    ParameterValue::List( ref list ) =>
                        Err(format!("Line {} : Parameter \"{}\" is not string", list.line, name)),
                }
            },
            None=>Err(format!("Line {} : Map has no parameter \"{}\"", self.line, name)),
        }
    }

    pub fn getStringAs<'a,T>( &'a self, name:&'a str ) -> Result<T,String> where T: FromStr{
        match self.params.get( name ){
            Some( pv ) => {
                match *pv {
                    ParameterValue::String{ line, ref value } =>{
                        match value.parse::<T>() {
                            Ok( p ) => Ok( p ),
                            Err( e )=>Err( format!("Line {} : Can not parse parameter \"{}\"",line,  name)),
                        }
                    },
                    ParameterValue::Map( ref map ) =>
                        Err(format!("Line {} : Parameter \"{}\" is not string", map.line, name)),
                    ParameterValue::List( ref list ) =>
                        Err(format!("Line {} : Parameter \"{}\" is not string", list.line, name)),
                }
            },
            None=>Err(format!("Line {} : Map has no parameter \"{}\"", self.line, name)),
        }
    }
}

pub struct List{
    line:usize,
    elements:Vec<ParameterValue>,
}

impl List{
    fn parse( cur:& mut TextCursor, listLine: usize ) -> Result<List, String>{
        let mut elements=Vec::new();

        loop{
            let line=cur.line;

            let elem=match try!(Lexeme::next( cur )) {
                Lexeme::Bracket( '{') =>
                    ParameterValue::Map( try!(Map::parse( cur, '}', line )) ),
                Lexeme::Bracket( '[') =>
                    ParameterValue::List( try!(List::parse( cur, line )) ),
                Lexeme::String( s ) =>
                    ParameterValue::String{line:line, value:s },
                Lexeme::Bracket(']') =>
                    break,
                Lexeme::NewLine | Lexeme::Comma =>
                    return Err(format!("Line {} : Parameter has been missed",line)),
                _=>
                    return Err( format!("Line {} : Expected string(\"..\"), list([...]), map(_.._)",line)),
            };

            elements.push(elem);

            match try!(Lexeme::next( cur )){
                Lexeme::Bracket(']') =>
                    break,
                Lexeme::NewLine | Lexeme::Comma => {},
                _=>return Err(format!("Line {} : Expected new line or , ]",cur.line)),
            }
        }

        Ok( List{
                line:listLine,
                elements:elements,
            }
        )
    }

    pub fn iter<'a>( &'a self ) -> Iter<'a,ParameterValue> {
        self.elements.iter()
    }


}

struct TextCursor<'a>{
    text:&'a String,
    it:Chars<'a>,
    pos:usize,
    lineBegin:usize,
    line:usize,
    ch:char,
}

impl<'a> TextCursor<'a>{
    fn new(text:&'a String) -> TextCursor<'a>{
        TextCursor{
            text:text,
            it:text.chars(),
            pos:0,
            lineBegin:0,
            line:1,
            ch:'\0',
        }
    }

    fn next(&mut self) -> char{
        self.ch=match self.it.next(){
            None=>'\0',
            Some(ch)=>{
                if ch=='\n' {
                    self.lineBegin=self.pos;
                    self.line+=1;
                }

                self.pos+=1;

                ch
            }
        };

        self.ch
    }
}

fn getLine( text:&String, lineBegin:usize ) -> String{
    let mut line=String::with_capacity(80);
    for ch in text.chars().skip(lineBegin).take_while(|c| *c!='\n' && *c!='\0') {
        line.push( ch );
    }

    line
}

#[derive(PartialEq, PartialOrd)]
enum Lexeme{
    EOF,
    String(String),
    Set,
    Comma,
    NewLine,
    Bracket(char),
}

impl Lexeme {
    fn next( cur:&mut TextCursor ) -> Result< Lexeme, String >{
        cur.next();

        loop {
            if cur.ch==' ' || cur.ch=='\t' {
                cur.next();
            }else if cur.ch=='/' {
                if cur.next()!='/' {
                    return Err(format!("Line {} : Comment must begin with \"//\"",cur.line));
                }

                while cur.ch!='\n' {
                    cur.next();
                }
            }else{
                break;
            }
        }

        match cur.ch {
            '\n'=>Ok( Lexeme::NewLine ),
            '\0'=>Ok( Lexeme::EOF ),
            ':' | '=' =>Ok( Lexeme::Set ),
            ','=>Ok( Lexeme::Comma ),
            '{' | '}' | '[' | ']' =>Ok( Lexeme::Bracket(cur.ch) ),
            '"' | '\'' => {
                let beginChar=cur.ch;
                let mut string=String::with_capacity(32);
                let mut isShielding=false;

                loop{
                    let ch=cur.next();

                    if ch=='\0' {
                        return Err(format!("Line {} : Expected \"{}\" at the end of string \"{}\"", cur.line, beginChar, string));
                    }else if isShielding {
                        match ch {
                            '"'=>string.push('"'),
                            '\''=>string.push('\''),
                            '\\'=>string.push('\\'),
                            'n'=>string.push('\n'),
                            _=>string.push(ch),
                        }

                        isShielding=false;
                    }else{
                        match ch {
                            '"' | '\''=>{
                                if beginChar==ch {
                                    break;
                                }else{
                                    string.push(ch);
                                }
                                string.push(ch);
                            },
                            '\\'=>isShielding=true,
                            '\n'=>return Err(format!("Line {} : Expected \"{}\" at the end of \"{}\"", cur.line, beginChar, string)),
                            _=>string.push(ch),
                        }
                    }
                }

                Ok( Lexeme::String(string) )
            },
            _=>return Err(format!("Line {} : You must write all strings in \"\"", cur.line)),
        }
    }
}

pub fn parse<T,F>( text:&String, process:F) -> Result<T, String> where F:FnOnce(Map) -> Result<T, String>{
    let mut cur=TextCursor::new(text);

    let map=try!(Map::parse( &mut cur, '\0', 0));

    process(map)
}
