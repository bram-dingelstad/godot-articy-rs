use gdnative::prelude::*;
use articy::{Interpreter, Outcome, types::{ArticyFile, Id}};

#[derive(NativeClass)]
#[inherit(Node)]
#[register_with(Self::register_signals)]
pub struct Articy {
    interpreter: Option<Interpreter>
}

// TODO: Implement signals

#[methods]
impl Articy {
    fn new(_base: &Node) -> Self {
        Articy {
            interpreter: None
        }
    }

    fn register_signals(builder: &ClassBuilder<Self>) {
        builder
            .signal("started")
            .done();

        builder
            .signal("line")
            .with_param(
                "line",
                VariantType::Dictionary
            )
            .done();

        builder
            .signal("choices")
            .with_param(
                "choices",
                VariantType::VariantArray
            )
            .done();

        builder
            .signal("stopped")
            .done();
    }

    #[method]
    fn load(&mut self, bytes: PoolArray<u8>) {
        let bytes = bytes.to_vec();
        // TODO: Do better error_handling
        let file: ArticyFile = serde_json::from_slice(&bytes)
                .expect("to be able to parse articy data");

        self.interpreter = Some(Interpreter::new(file))
    }

    #[method]
    fn start(&mut self, #[base] owner: &Node, id: String)  {
        match &mut self.interpreter {
            Some(interpreter) => { 
                interpreter.start(Id(id)).unwrap(); 

                let dictionary = Dictionary::new();
                let model = interpreter.get_current_model().unwrap();

                dictionary.insert("line", model.properties.text.as_ref().unwrap_or(&"no line found".to_string()));
                dictionary.insert("id", model.properties.id.to_inner());

                owner.emit_signal("line", &[Variant::new(dictionary)]);
            },
            _ => {} // FIXME: Add error handling
        }
    }

    #[method]
    fn advance(&mut self, #[base] owner: &Node) {
        match &mut self.interpreter {
            Some(interpreter) => match interpreter.advance() {
                Ok(outcome) => match outcome {
                    Outcome::Advanced(model) => {
                        let dictionary = Dictionary::new();

                        dictionary.insert("line", model.properties.text.as_ref().unwrap_or(&"no line found".to_string()));
                        dictionary.insert("id", model.properties.id.to_inner());

                        owner.emit_signal("line", &[Variant::new(dictionary)]);
                    },
                    Outcome::WaitingForChoice(choices) => {
                        let array = VariantArray::new();
                        for choice in choices {
                            let dictionary = Dictionary::new();

                            dictionary.insert("label", choice.properties.text.as_ref().unwrap_or(&"no choice label found".to_string()));
                            dictionary.insert("id", choice.properties.id.to_inner());

                            array.push(dictionary);
                        }
                        
                        owner.emit_signal("choices", &[Variant::new(array)]);
                    },
                    Outcome::Stopped | Outcome::EndOfDialogue => { owner.emit_signal("stopped", &[]); }
                },
                Err(_error) => {} // FIXME: Add error handling
            },
            _ => {} // FIXME: Add error handling
        }
    }


    #[method]
    fn choose(&mut self, index: usize) {
        match &mut self.interpreter {
            Some(interpreter) => { 
                interpreter.choose(index).unwrap();
            },
            None => {} // FIXME: Make error
        }
    }
}

fn init(handle: InitHandle) {
    handle.add_class::<Articy>();
}

godot_init!(init);
