use {
    crate::{
        audio_component_factory,
        audio::*,
        makepad_component::*,
        makepad_platform::*
    },
};

live_register!{
    Mixer: {{Mixer}} {
    }
}

//enum ToUI {}
enum FromUI {}

#[derive(Live)]
#[live_register(audio_component_factory!(Mixer))]
struct Mixer {
    #[rust] inputs: ComponentMap<LiveId, AudioComponentRef>,
    #[rust] from_ui: FromUISender<FromUI>,
}

impl LiveHook for Mixer {
    fn apply_value_instance(&mut self, cx: &mut Cx, from: ApplyFrom, index: usize, nodes: &[LiveNode]) -> usize {
        self.inputs.get_or_insert(cx, nodes[index].id, | cx | {AudioComponentRef::new(cx)})
            .apply(cx, from, index, nodes)
    }
    
    fn after_apply(&mut self, _cx: &mut Cx, from: ApplyFrom, _index: usize, _nodes: &[LiveNode]) {
        // so.. alright.. if we have a file_id we can gc the inputs
        if from.is_from_doc() {
            self.inputs.retain_visible();
        }
    }
}

struct Node {
    _from_ui: FromUIReceiver<FromUI>,
    buffer: AudioBuffer,
    inputs: Vec<Box<dyn AudioGraphNode + Send >>
}

// ok so how do we spawn this shit up.

impl AudioGraphNode for Node {
    fn handle_midi_1_data(&mut self, data: Midi1Data) {
        for input in &mut self.inputs {
            input.handle_midi_1_data(data);
        }
    }
    
    fn render_to_audio_buffer(&mut self, time: AudioTime, outputs: &mut [&mut AudioBuffer], _inputs: &[&AudioBuffer]) {
        let output = &mut outputs[0];
        self.buffer.resize_like(*output);
        output.zero();
        for i in 0..self.inputs.len() {
            let input = &mut self.inputs[i];
            input.render_to_audio_buffer(time, &mut [&mut self.buffer], &[]);
            for c in 0..output.channel_count() {
                let out_channel = output.channel_mut(c);
                let in_channel = self.buffer.channel(c);
                for j in 0..out_channel.len() {
                    out_channel[j] += in_channel[j];//*0.1;
                }
            }
        }
    }
}


impl AudioComponent for Mixer {
    fn get_graph_node(&mut self) -> Box<dyn AudioGraphNode + Send> {
        
        self.from_ui.new_channel();
        let mut inputs = Vec::new();
        for input in self.inputs.values_mut() {
            if let Some(input) = input.as_mut() {
                inputs.push(input.get_graph_node());
            }
        }
        Box::new(Node {
            inputs,
            buffer: AudioBuffer::default(),
            _from_ui: self.from_ui.receiver()
        })
    }
    
    fn handle_event_with_fn(&mut self, cx: &mut Cx, event: &mut Event, dispatch_action: &mut dyn FnMut(&mut Cx, AudioComponentAction)) {
        for input in self.inputs.values_mut() {
            if let Some(input) = input.as_mut() {
                input.handle_event_with_fn(cx, event, dispatch_action)
            }
        }
    }
}

