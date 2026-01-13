use std::net::SocketAddr;

use bevy::prelude::*;
use math::{noise::worley::Worley3, rng::Permutation};

use crate::net::Server;
use world::{Voxel, World};
