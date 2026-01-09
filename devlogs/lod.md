
# Why LOD Probably Wont Happen

LOD (Level of Detail) refers to rendering chunks that are farther away with a simplified pipeline. The idea is that this would allow users to have much farther render distances on much slower hardware. 

## Its not about your GPU

Bedrock Edition Minecraft has had 96-chunk render distances for a few years now. For alot of these new GPUs, really high render distances are just kind of trivial. However, this is only realistically achieveable _in singleplayer_. But nobody plays singleplayer. In order for high render distances to be a selling point for our game, they have to be possible _in multiplayer_.

The reason this is so difficult is because of **IO Bottlenecks**. It is very expensive to load, compress, transmit, and decompress world data. We're able to avoid
