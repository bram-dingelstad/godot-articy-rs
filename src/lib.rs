use articy::{
    types::{self, File as ArticyFile, Id, Model},
    Interpreter as ArticyInterpreter, Outcome,
};
use gdnative::api::PackedDataContainer;
use gdnative::prelude::*;
use serde_json::Value;
use std::rc::Rc;

#[derive(NativeClass, Debug, Default)]
#[inherit(Node)]
pub struct Database {
    #[property]
    articy_resource: Option<Ref<PackedDataContainer>>,
    pub file: Option<Rc<ArticyFile>>,
}

#[derive(ToVariant, Debug)]
pub struct Dialogue {
    id: String,
    name: String,
}

#[derive(ToVariant, Debug)]
pub enum Error {
    DatabaseNotSetup,
    InterpreterNotSetup,
    ArticyError, //FIXME: Add articy in tuple enum and handle ToVariant requirement (ArticyError),
}

impl From<&types::Model> for Dialogue {
    fn from(model: &types::Model) -> Dialogue {
        match model {
            Model::Dialogue {
                id, display_name, ..
            } => Dialogue {
                id: id.to_inner(),
                name: display_name.to_owned(),
            },
            _ => todo!("Implement Failure"),
        }
    }
}

#[methods]
impl Database {
    fn new(_base: &Node) -> Self {
        Default::default()
    }

    #[method]
    fn _ready(&mut self) {
        if let Some(resource) = &self.articy_resource {
            let resource = unsafe { resource.assume_safe() };

            let data = resource
                .get("__data__")
                .to::<PoolArray<u8>>()
                .expect("__data__ to be of type PoolArray<u8> (PoolByteArray)");

            self.load(data);
        }
    }

    #[method]
    fn load(&mut self, bytes: PoolArray<u8>) {
        let file: ArticyFile =
            serde_json::from_slice(&bytes.to_vec()).expect("to be able to parse articy data");

        self.file = Some(Rc::from(file));
    }

    #[method]
    fn get_available_dialogues(&self) -> Vec<Dialogue> {
        // TODO: Make this relative to the current Flow context
        if let Some(file) = &self.file {
            let flow_id = file
                .get_default_package()
                .models
                .first()
                .expect("to find models in the default package")
                .id();

            let list = file
                .get_dialogues_in_flow(&flow_id)
                .iter()
                .map(|dialogue| Dialogue::from(*dialogue))
                .collect::<Vec<Dialogue>>();

            list
        } else {
            vec![]
        }
    }

    #[method]
    fn get_model(&self, id: String) -> Option<ArticyModel<'_>> {
        self.file
            .as_ref()?
            .get_default_package()
            .models
            .iter()
            .find_map(|model| {
                if model.id().to_inner() == id {
                    Some(ArticyModel(model))
                } else {
                    None
                }
            })
    }

    #[method]
    fn get_models_of_type(&self, kind: String) -> Result<Vec<ArticyModel<'_>>, Error> {
        Ok(self
            .file
            .as_ref()
            .ok_or(Error::DatabaseNotSetup)?
            .get_models_of_type(&kind)
            .iter()
            .map(|model| ArticyModel(model))
            .collect::<Vec<ArticyModel<'_>>>())
    }
}

#[derive(NativeClass, Default)]
#[inherit(Node)]
#[register_with(Self::register_signals)]
struct Interpreter {
    #[property]
    database_path: Option<NodePath>,

    interpreter: Option<ArticyInterpreter>,
}

#[methods]
impl Interpreter {
    fn new(_base: &Node) -> Self {
        Default::default()
    }

    fn register_signals(builder: &ClassBuilder<Self>) {
        builder.signal("started").done();

        builder
            .signal("line")
            .with_param("line", VariantType::Dictionary)
            .done();

        builder
            .signal("choices")
            .with_param("choices", VariantType::VariantArray)
            .done();

        builder.signal("stopped").done();
    }

    #[method]
    fn _ready(&mut self, #[base] owner: &Node) {
        if let Some(path) = &self.database_path {
            let node = owner
                .get_node(path.to_godot_string())
                .expect("To find node for path");

            let file = unsafe {
                node.assume_safe()
                    .cast_instance::<Database>()
                    .expect("to find a Database type from the Articy integration")
                    .map(|data, _base| data.file.clone())
                    .expect("to get `file` mapped from the Articy Database Rust type")
            }
            .expect("for the Articy Database to have a file loaded");

            self.interpreter = Some(ArticyInterpreter::new(file));

            godot_print!("Loaded Articy Interpreter with \"{path:?}\" as a source!");
        }
    }

    #[method]
    fn start(&mut self, #[base] owner: &Node, id: String) -> Result<(), Error> {
        let interpreter = self
            .interpreter
            .as_mut()
            .ok_or(Error::InterpreterNotSetup)?;

        interpreter.start(Id(id)).ok().ok_or(Error::ArticyError)?;

        let dictionary = Dictionary::new();
        let model = interpreter
            .get_current_model()
            .ok()
            .ok_or(Error::ArticyError)?;

        match model {
            Model::DialogueFragment {
                text,
                id,
                speaker,
                technical_name,
                ..
            } => {
                dictionary.insert("line", text.to_owned());
                dictionary.insert("id", id.to_inner());
                dictionary.insert("speaker", speaker.to_inner());
                dictionary.insert("technical_name", technical_name.to_owned());

                owner.emit_signal("line", &[Variant::new(dictionary)]);
            }
            other_model => {
                godot_error!("Implement start translation for {other_model:?}")
            }
        }

        Ok(())
    }

    #[method]
    fn advance(&mut self, #[base] owner: &Node) -> Result<(), Error> {
        let interpreter = self
            .interpreter
            .as_mut()
            .ok_or(Error::InterpreterNotSetup)?;

        match interpreter.advance() {
            Ok(outcome) => match outcome {
                Outcome::Advanced(Model::DialogueFragment {
                    id,
                    text,
                    speaker,
                    technical_name,
                    ..
                }) => {
                    let dictionary = Dictionary::new();

                    dictionary.insert("id", id.to_inner());
                    dictionary.insert("line", text.to_owned());
                    dictionary.insert("speaker", speaker.to_inner());
                    dictionary.insert("technical_name", technical_name.to_owned());

                    owner.emit_signal("line", &[Variant::new(dictionary)]);
                }
                Outcome::Advanced(other_model) => {
                    godot_error!("Implement advance translation for {other_model:?}")
                }
                Outcome::WaitingForChoice(choices) => {
                    let array = VariantArray::new();
                    for choice in choices {
                        let dictionary = Dictionary::new();
                        match choice {
                            Model::DialogueFragment { text, id, .. } => {
                                dictionary.insert("label", text.to_owned());
                                dictionary.insert("id", id.to_inner());

                                array.push(dictionary);
                            }
                            other_model => {
                                godot_error!("Implement choice translation for {other_model:?}")
                            }
                        }
                    }

                    owner.emit_signal("choices", &[Variant::new(array)]);
                }
                Outcome::Stopped | Outcome::EndOfDialogue => {
                    owner.emit_signal("stopped", &[]);
                }
            },
            Err(error) => godot_error!("Interpreter.advance() returned an error: {error:#?}"),
        }

        Ok(())
    }

    #[method]
    fn choose(&mut self, id: String) -> Result<(), Error> {
        let interpreter = self
            .interpreter
            .as_mut()
            .ok_or(Error::InterpreterNotSetup)?;

        interpreter.choose(Id(id)).ok().ok_or(Error::ArticyError)?;

        Ok(())
    }
}

struct ArticyModel<'a>(&'a Model);

impl ToVariant for ArticyModel<'_> {
    fn to_variant(&self) -> Variant {
        let dictionary = Dictionary::new();

        match self.0 {
            Model::Custom(key, value) => {
                dictionary.insert("type", key);

                match value {
                    Value::Object(map) => {
                        for key in map.keys() {
                            match map.get(key) {
                                Some(Value::String(string)) => dictionary.insert(key, string),
                                _ => godot_error!(
                                    "Implement key-value type {:?} for ToVariant",
                                    value
                                ),
                            }
                        }
                    }
                    _ => godot_error!("Implement value type {:?} for ToVariant", value),
                }
            }
            _ => godot_error!("Implement type {:?} for ToVariant", self.0),
        }

        // dictionary.insert("label", fragment.text.clone());

        Variant::new(dictionary)
    }
}

fn init(handle: InitHandle) {
    handle.add_class::<Database>();
    handle.add_class::<Interpreter>();
}

godot_init!(init);
