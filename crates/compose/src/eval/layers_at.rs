//! Which layers lay something down at one texel, for the viewport's Pick layer tool

use super::{
    BlendMode, Channel, Content, EvaluationInputs, Inputs, LayerId, NCH, NO_TRI, Node, ProjectorDepthCache, TILE, Texel, evaluation_channels,
    evaluation_fields, plan,
};

/// Below this weight a layer leaves the texel unchanged (one 16-bit step)
const SMALLEST_WEIGHT: f32 = 1.0 / 65535.0;

/// The paint and fill layers (and instances) that lay something down at texel (x, y) of texture set `inputs.ts` in any of `shown` channels, top first, each with the shown channels it changes there. Hidden layers, layers masked out there (by their own mask or a folder's) and channels at zero opacity count as laying nothing down
pub fn layers_at(inputs: &Inputs, (x, y): (u32, u32), shown: &[Channel]) -> Vec<(LayerId, Vec<Channel>)> {
    let Ok(set) = inputs.project.texture_set(inputs.ts) else { return Vec::new() };
    let gbuf = inputs.gbuf;
    let covered = x < gbuf.width && y < gbuf.height && gbuf.tri.get((y * gbuf.width + x) as usize).is_some_and(|tri| *tri != NO_TRI);
    if !covered {
        return Vec::new();
    }

    let projectors = ProjectorDepthCache::default();
    let inputs = &EvaluationInputs::new(inputs, &projectors);
    let channels = evaluation_channels(inputs, set.channel_list());

    // Filters and anchors can read texels outside this one's tile: the fields cover what it reads
    let (tile_x, tile_y) = (x / TILE, y / TILE);
    let fields = evaluation_fields(inputs, &channels, &[(tile_x, tile_y)]);
    let nodes = plan(inputs, &set.stack, &channels, tile_x, tile_y, 0, Some(&fields));
    let offset = ((y % TILE) * TILE + x % TILE) as usize;
    let mut texel = Texel::new(inputs, x, y, offset, 0.0);

    // The whole stack runs first, so masks and fills that read anchors find them
    let mut values = Channel::ALL.map(Channel::default_value);
    texel.eval(&nodes, &mut values);

    let mut found = Vec::new();
    texel.layers_laying_down(&nodes, [1.0; NCH], &shown, &mut found);

    // The stack runs bottom first
    found.reverse();
    found
}

impl Texel<'_, '_> {
    /// Push the leaf layers among `nodes` (bottom first) that lay something down in a `shown` channel, with those channels, each channel weighted by `weights` (the opacity and mask of the folders holding them)
    fn layers_laying_down(&mut self, nodes: &[Node], weights: [f32; NCH], shown: &[Channel], found: &mut Vec<(LayerId, Vec<Channel>)>) {
        for node in nodes {
            let mask = self.node_mask(node);
            if mask <= 0.0 {
                continue;
            }

            let mut node_weights = [0.0; NCH];
            for chan in node.chans.iter().filter(|chan| shown.iter().any(|channel| channel.index() == chan.ch)) {
                if let (Some(weight), Some(slot)) = (weights.get(chan.ch), node_weights.get_mut(chan.ch)) {
                    *slot = weight * chan.opacity * mask;
                }
            }

            match &node.content {
                Content::Group(children) => self.layers_laying_down(children, node_weights, shown, found),
                content => {
                    let changed: Vec<Channel> = shown
                        .iter()
                        .copied()
                        .filter(|channel| {
                            node.chans.iter().filter(|chan| chan.ch == channel.index()).any(|chan| {
                                let weight = node_weights.get(chan.ch).copied().unwrap_or(0.0);

                                // Replace lays its value down at full weight wherever the layer has content
                                weight > SMALLEST_WEIGHT
                                    && self
                                        .content(content, chan)
                                        .is_some_and(|(_, coverage)| chan.blend == BlendMode::Replace || weight * coverage > SMALLEST_WEIGHT)
                            })
                        })
                        .collect();
                    if !changed.is_empty() {
                        found.push((node.draw.0, changed));
                    }
                }
            }
        }
    }
}
