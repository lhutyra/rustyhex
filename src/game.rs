// Copyright 2014 Dawid Ciężarkiewicz
// See LICENSE file for more information

use creature::{Creature};
use creature::{Race,Human,Scout,Grunt,Heavy};
use hex2d;
use hex2d::{Point,Position,Direction};
use hex2d::{Forward,Backward};
use map::{Tile,Map};
use map::{Wall,Floor,GlassWall,Sand};
use std::rand;
use std::rand::Rng;
use std::cell::{RefCell};
use std::rc::{Rc,Weak};
use std::vec::Vec;

pub type Creatures<'a> = Vec<Weak<RefCell<Creature<'a>>>>;

pub struct GameState<'a> {
    pub map : Box<Map<'a>>,
    pub player : Option<Weak<RefCell<Creature<'a>>>>,
    rng : rand::TaskRng,
    creatures: Creatures<'a>,
}

#[deriving(Show)]
pub enum Action {
    Run(Direction),
    Move(Direction),
    Turn(Direction),
    Melee(Direction),
    Use,
    Wait
}

impl<'a> GameState<'a> {
    pub fn new() -> GameState<'a> {
        let map = box hex2d::Map::new(100, 100, Tile {
            tiletype: Floor,
            creature: None,
        }
        );
        GameState {
            player: None,
            rng: rand::task_rng(),
            map: map,
            creatures: Vec::new(),
        }
    }

    fn spawn(&mut self, cr : Box<Creature>) -> Option<Rc<RefCell<Creature>>>  {
        if !self.map.at(*cr.p()).is_passable() {
            return None;
        }

        let p = *cr.p();
        let rc = Rc::new(RefCell::new(*cr));
        self.map.mut_at(p).creature = Some(rc.clone());
        self.creatures.push(rc.downgrade());
        Some(rc.clone())
    }


    fn spawn_random(&mut self, player : bool, race : Race) -> Rc<RefCell<Creature>> {
        loop {
            let pos = self.map.wrap(self.rng.gen::<Position>());
            let cr = box Creature::new(&*self.map, pos, player, race);
            match self.spawn(cr) {
                Some(cr) => return cr,
                None => {}
            }
        }
    }

    fn move_creature_if_possible(&mut self, cr : &mut Creature, pos : Position) {
        let cr_p = *cr.p();
        let pos_p = pos.p;
        if pos_p == cr_p {
            cr.pos_set(&*self.map, pos);
            return;
        }
        if !self.map.at(pos_p).is_passable() {
            return;
        }

        match self.map.at(pos_p).creature {
            Some(_) => { },
            None => {
                self.map.mut_at(pos_p).creature = self.map.at(cr_p).creature.clone();
                self.map.mut_at(cr_p).creature = None;
                cr.pos_set(&*self.map, pos);
            }
        }
    }

    pub fn tick(&mut self) {
        let mut creatures = self.creatures.clone();

        for cr in creatures.iter() {
            let creature = cr.upgrade();

            match creature.as_ref().map(|cr| cr.borrow_mut())  {
                Some(mut cr) => {
                    if cr.needs_action() {
                        assert!(!cr.is_player());
                        cr.update_los(&*self.map);
                    }
                    let action = cr.tick(&*self.map);
                    match action {
                        Some(action) => {
                            self.perform_action(&mut *cr, action);
                            cr.action_done();
                        },
                        None => {}
                    }
                },
                None => { }
            };
        }

        creatures.retain(
            |rc| {
                let rc = rc.upgrade();
                match rc.as_ref().map(|cr| cr.borrow()) {
                    Some(cr) => {
                        cr.is_alive()
                    },
                    None => false
                }
            }
            );
        self.creatures = creatures;
    }

    pub fn perform_action(&mut self, cr : &mut Creature, action : Action) {
        let old_pos = *cr.pos();
        cr.pos_prev_set(&*self.map, old_pos);

        match action {
            Turn(Forward)|Turn(Backward) => fail!("Illegal move"),
            Move(dir)|Run(dir) => {
                let pos = Position{ p: self.map.wrap(cr.p() + (cr.pos().dir + dir)), dir: cr.pos().dir };
                self.move_creature_if_possible(cr, pos)
            },
            Turn(dir) => {
                let pos = self.map.wrap(cr.pos() + dir);
                self.move_creature_if_possible(cr, pos)
            },
            Melee(dir) => {
                let target_p = self.map.wrap(cr.p() + (cr.pos().dir + dir));
                let target = self.map.mut_at(target_p).creature.as_ref().
                    map(|cr| cr.clone());
                if target.is_some() {
                    let target = target.unwrap();
                    let target = &mut *target.borrow_mut();
                    target.attacked_by(cr);
                    cr.attacked(target);

                    if !target.is_alive() {
                        self.map.mut_at(target_p).creature = None;
                    }
                }
            },
            _ => { }
        }
    }

    pub fn randomize_map(&mut self) {
        let height = self.map.height() as int;
        let width = self.map.width() as int;
        let area = width * height;

        for _ in range(0, area / 12) {
            let p = self.rng.gen::<Point>();
            let p = self.map.wrap(p);

            let t = match self.rng.gen_range(0u, 6) {
                0 => GlassWall,
                1 => Sand,
                _ => Wall
            };

            self.map.mut_at(p).tiletype = t;
            for &dir in hex2d::all_directions.iter() {
                let p = self.map.wrap(p + dir);
                self.map.mut_at(p).tiletype = t;
            }
        }

        for x in range(0i, width) {
            let p = Point::new(x, 0);
            self.map.mut_at(p).tiletype = Wall;
            let p = Point::new(x, height - 1);
            self.map.mut_at(p).tiletype = Wall;
        }

        for y in range(0i, height) {
            let p = Point::new(0, y);
            self.map.mut_at(p).tiletype = Wall;
            let p = Point::new(width - 1, y);
            self.map.mut_at(p).tiletype = Wall;
        }



        for _ in range(0, area / 200) {
            self.spawn_random(false, Scout);
        }

        for _ in range(0, self.map.width() * self.map.height() / 400) {
            self.spawn_random(false, Grunt);
        }

        for _ in range(0, self.map.width() * self.map.height() / 800) {
            self.spawn_random(false, Heavy);
        }

        let p = self.spawn_random(true, Human);

        self.player = Some(p.downgrade());
    }

    pub fn update_player_los(&self) {
        let pl = self.player.as_ref().and_then(|pl| pl.upgrade());
        if pl.is_some() {
            let pl = pl.unwrap();
            pl.borrow_mut().update_los(&*self.map);
        }
    }
}