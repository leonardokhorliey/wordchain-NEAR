
use near_contract_standards::fungible_token::receiver::FungibleTokenReceiver;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::{LazyOption, LookupMap, Vector};
use near_sdk::json_types::U128;
use near_sdk::serde::Serialize;
use near_sdk::{env, log, near_bindgen, AccountId, Balance, PanicOnDefault, BorshStorageKey, PromiseOrValue, ext_contract, require};

pub const DAY_TO_MS: u64 = 86400000;

#[derive(Eq, PartialEq)]
#[derive(Clone, BorshDeserialize, BorshSerialize, Serialize)]
#[serde(crate = "near_sdk::serde")]
pub enum TournamentType {
    PUBLIC,
    PRIVATE,
    COUNTRY_BASED,
}

#[derive(Eq, PartialEq)]
#[derive(Clone, BorshDeserialize, BorshSerialize, Serialize)]
#[serde(crate = "near_sdk::serde")]
pub enum TournamentState {
    PENDING_VOLUME,
    ACTIVE,
    DELETED,
    CLOSED
}

#[derive(Eq, PartialEq)]
#[derive(Clone, BorshDeserialize, BorshSerialize, Serialize)]
#[serde(crate = "near_sdk::serde")]
enum PlayerState {
    ACTIVE,
    BLACKLISTED,
}

#[derive(Clone, BorshDeserialize, BorshSerialize, Serialize)]
#[serde(crate = "near_sdk::serde")]
struct GameType {
    identifier: String,
    max_score: u64,
}

#[derive(Debug, Eq, Ord, PartialEq, PartialOrd)]
#[derive(Clone, BorshDeserialize, BorshSerialize, Serialize)]
#[serde(crate = "near_sdk::serde")]
struct TournamentPlayer {
    id: U128,
    account_id: AccountId,
    stake_amount: Balance,
    score: u64,
    number_of_games_played: u64,
    join_date: u64
}

#[derive(BorshDeserialize, BorshSerialize, Serialize)]
#[serde(crate = "near_sdk::serde")]
struct Tournament {
    id: U128,
    name: String,
    tournament_key: String,
    game_type_id: String,
    owner: AccountId,
    minimum_stake: Balance,
    created_at: u64,
    total_stake: Balance,
    country: String,
    ft_address: AccountId,
    tournament_deadline: u64,
    tournament_type: TournamentType,
    status: TournamentState,
    players: Vec<TournamentPlayer>
}

#[ext_contract(ext_token_contract)]
trait StableCoin {
    fn ft_transfer(&self, to: &AccountId, amount: Balance, memo: Option<String>);

}

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize)]
struct Wordchain {
    paused: bool,
    owner: AccountId,
    pending_owner: AccountId,
    min_tournament_players: u8,
    stake_payouts: LookupMap<AccountId, Balance>,
    percentage_stake_commission: u64,
    game_types: Vector<GameType>,
    tournaments_to_players: LookupMap<U128, Vector<AccountId>>,
    tournaments: Vector<Tournament>,
    supported_countries: Vector<String>,
    stakes: LookupMap<AccountId, LookupMap<AccountId, Balance>>,

}

#[near_bindgen]
impl Wordchain {

    #[init]
    pub fn new(percentage_stake_commission: u64, countries: String, min_tournament_players: u8) -> Self {
        require!(min_tournament_players > 3, "Minimum number of players must be greater then 3");
        require!(percentage_stake_commission > 1000, "Commission must be at least 10 percent");
        require!(countries.replace("|", "").len() > 2, "Enter at least one valid country code.");
        let countries_split = countries.split("|").collect::<Vec<&str>>();

        let mut supported_countries = Vector::new(b"c");

        countries_split.into_iter().for_each(|cont| supported_countries.push(&cont.to_string()));

        Self {
            paused: bool::default(),
            owner: env::signer_account_id(),
            pending_owner: env::current_account_id(),
            min_tournament_players,
            stake_payouts: LookupMap::new(b"d"),
            percentage_stake_commission,
            game_types: Vector::new(b"g"),
            tournaments_to_players: LookupMap::new(b"p"),
            tournaments: Vector::new(b"t"),
            supported_countries,
            stakes: LookupMap::new(b"s")
        }
    }

    /// .
    pub fn create_tournament(&mut self, 
        name: String, 
        tournament_key: String,
        game_type_id: String, 
        form: TournamentType, 
        interval: u64, 
        minimum_stake: U128, 
        ft_address: AccountId,
        country: Option<String>) -> Tournament {

        match form {
            TournamentType::COUNTRY_BASED => {
                require!(country.is_some(), "Country based tournament requires a country to be passed");
                require!(self.check_supported_country(country.clone().unwrap_or_default()), "Country code entered is not supported");
            },
            _ => {}
        }

        require!(self.get_tournament_by_key_or_name(tournament_key.clone(), name.clone()).is_none(), "Tournament with provided key or name already exists");
        require!(self.get_gametypes(Some(game_type_id.clone())).len() > 0, "No tournament with provided game type");

        if let Some(stakings) = Some(self.stakes.get(&env::predecessor_account_id())) {
            let mut stakes = stakings.unwrap();
            let ft_stake = stakes.get(&ft_address).unwrap_or_else(|| env::panic_str("No stake made"));

            if ft_stake < minimum_stake.0 {
                env::panic_str("You must have staked at least the minimum stake before creating tournament");
            } else {

                let tournament_id = U128::from((self.tournaments.len() as u128) + 1);

                let mut players_ = Vec::new();

                if env::predecessor_account_id() != self.owner {
                    players_.push(TournamentPlayer {
                        id: U128::from(1),
                        account_id: env::predecessor_account_id(),
                        stake_amount: minimum_stake.0,
                        score: 0,
                        number_of_games_played: 0,
                        join_date: env::block_timestamp_ms(),
                    });
                }

                let tournament = Tournament {
                    id: tournament_id,
                    name,
                    tournament_key,
                    game_type_id,
                    owner: env::predecessor_account_id(),
                    minimum_stake: minimum_stake.0,
                    total_stake: 0,
                    country: country.unwrap_or_default(),
                    ft_address,
                    created_at: env::block_timestamp_ms(),
                    tournament_deadline: env::block_timestamp_ms() + (interval * DAY_TO_MS),
                    tournament_type: form,
                    status: TournamentState::PENDING_VOLUME,
                    players: players_
                };

                self.tournaments.push(&tournament);

                stakes.insert(&tournament.ft_address, &0);
                self.stakes.insert(&env::predecessor_account_id(), &stakes);

                tournament
            }
        } else {
            env::panic_str("No stake made");
        }

    }


    /// .
    pub fn join_tournament(&mut self,
        tournament_id: U128,
        country: String,
        tournament_key: Option<String>,
    ) {
        let mut tournament = self.tournaments.get(tournament_id.0 as u64).unwrap_or_else(|| env::panic_str("Tournament with provided ID does not exist"));
        require!(tournament.owner != env::predecessor_account_id(), "Tournament owner can not join the tournament");
        require!(tournament.tournament_deadline > env::block_timestamp_ms(), "Tournament exceeded the deadline");

        match tournament.tournament_type {
            TournamentType::PRIVATE => {
                require!(tournament.tournament_key == tournament_key.unwrap_or_default(), "Invalid tournament key provided for a private tournament");
            },
            TournamentType::COUNTRY_BASED => {
                require!(tournament.country == country, "Invalid country");
            },
            _ => {}
        }

        if let Some(stakings) = Some(self.stakes.get(&env::predecessor_account_id())) {

            let mut stakes = stakings.unwrap();
            let ft_stake = stakes.get(&tournament.ft_address).unwrap_or_else(|| env::panic_str("No stake made"));

            if ft_stake < tournament.minimum_stake {
                env::panic_str("You must have staked at least the minimum stake before creating tournament");
            } else {
                let number_of_players = (tournament.players.len() as u128) + 1;

                let player_id = U128::from(number_of_players.clone());

                tournament.players.push(TournamentPlayer {
                    id: player_id,
                    account_id: env::predecessor_account_id(),
                    stake_amount: ft_stake,
                    score: 0,
                    number_of_games_played: 0,
                    join_date: env::block_timestamp_ms(),
                });

                if number_of_players >= self.min_tournament_players as u128 {
                    tournament.status = TournamentState::ACTIVE;
                }

                self.tournaments.replace(tournament_id.0 as u64, &tournament);
                stakes.insert(&tournament.ft_address, &0);
                self.stakes.insert(&env::predecessor_account_id(), &stakes);
            }

        } else {
            env::panic_str("No stake made");
        }
  
        
    }


    #[doc = r"Function to handle score update after playing a game"]
    pub fn publish_score(&mut self, tournament_id: U128, score: u8) {

        let mut tournament = self.tournaments.get(tournament_id.0 as u64).unwrap_or_else(|| env::panic_str("Tournament with provided ID does not exist"));
        require!(tournament.tournament_deadline > env::block_timestamp_ms(), "Tournament exceeded deadline");
        require!(tournament.status == TournamentState::ACTIVE, "Tournament can not be joined at this time");

        let gametype = self.get_gametypes(Some(tournament.game_type_id.clone()));
        require!(score as u64 <= gametype[0].max_score, "Score exceeds threshold for game");

        tournament.players.clone().into_iter().for_each(|mut player| {
            if player.account_id == env::predecessor_account_id() {
                player.score += score as u64;
                player.number_of_games_played += 1;
            }
        });


        self.tournaments.replace(tournament_id.0 as u64, &tournament);

    }


    //Admin level
    pub fn distribute_rewards(&mut self, tournament_id: U128) {

        let mut tournament = self.tournaments.get(tournament_id.0 as u64).unwrap_or_else(|| env::panic_str("Tournament with provided ID does not exist"));
        require!(self.owner == env::predecessor_account_id(), "Unauthorized");
        require!(tournament.tournament_deadline <= env::block_timestamp_ms(), "Tournament owner can not join the tournament");
        require!(tournament.status != TournamentState::CLOSED, "Tournament is already closed");

        let mut stake_payout_for_ft = self.stake_payouts.get(&tournament.ft_address).unwrap_or_default();

        match tournament.status {
            TournamentState::PENDING_VOLUME => {
                tournament.players.iter().for_each(|player| {
                    
                    if player.account_id != tournament.owner {
                        ext_token_contract::ext(tournament.ft_address.clone())
                        .ft_transfer(
                            &player.account_id,
                            player.stake_amount,
                            None
                        );

                    } else {

                        stake_payout_for_ft += player.stake_amount;
                        self.stake_payouts.insert(&tournament.ft_address, &stake_payout_for_ft);
                    }
                });

                tournament.status = TournamentState::CLOSED;
                self.tournaments.replace(tournament_id.0 as u64, &tournament);
            },
            TournamentState::DELETED => {},
            _ => {
                let commission = (self.percentage_stake_commission as u128 * tournament.total_stake) / 10000;
                stake_payout_for_ft += commission;
                self.stake_payouts.insert(&tournament.ft_address, &stake_payout_for_ft);

                let pay_value = tournament.total_stake - commission;

                let mut players = tournament.players.clone().into_iter().map(|player| player).collect::<Vec<TournamentPlayer>>();

                players.sort_by(|a, b| ((b.score*1000)/b.number_of_games_played).cmp(&((a.score*1000)/a.number_of_games_played)));

                let prizes = self.get_position_prizes();

                for i in 1..prizes.len() {
                    let val_to_pay = prizes[i] as u128 * pay_value;

                    ext_token_contract::ext(tournament.ft_address.clone())
                        .ft_transfer(
                            &players[i].account_id,
                            val_to_pay,
                            None
                        );
                }
                tournament.status = TournamentState::CLOSED;
                self.tournaments.replace(tournament_id.0 as u64, &tournament);
            }
        }

    }


    pub fn transfer_ownership(&mut self, new_owner: AccountId) {
        require!(self.owner == env::predecessor_account_id(), "Unauthorized");
        self.pending_owner = new_owner;
    }

    pub fn accept_ownership(&mut self) {
        require!(self.pending_owner == env::predecessor_account_id(), "Unauthorized");
        self.owner = env::predecessor_account_id();
        self.pending_owner = env::current_account_id();
    }

    pub fn pause_contract(&mut self) {
        require!(self.owner == env::predecessor_account_id(), "Unauthorized");
        require!(!self.paused, "Contract is paused");
        self.paused = true;
    }

    pub fn unpause_contract(&mut self) {
        require!(self.owner == env::predecessor_account_id(), "Unauthorized");
        require!(self.paused, "Contract is not paused");
        self.paused = false;
    }

    pub fn withdraw_value(&mut self, to: AccountId, ft_address: AccountId, amount: Option<U128>) -> Balance {

        require!(self.owner == env::predecessor_account_id(), "Unauthorized");
        let payout = self.stake_payouts.get(&ft_address).unwrap_or_default();
        match amount {
            Some(amt) => {
                let mut bal: u128 = 0;
                
                if payout < amt.0 {
                    env::panic_str("Confirm correct token address or withdraw amount below threshold");
                } else {
                    ext_token_contract::ext(ft_address.clone())
                        .ft_transfer(
                            &to,
                            amt.0,
                            None
                        );
                    
                    bal = payout - amt.0
                }
                bal

            },
            None => {
                ext_token_contract::ext(ft_address.clone())
                    .ft_transfer(
                        &to,
                        payout,
                        None
                    );

                0
            }
        }
    }

    pub fn add_supported_country(&mut self, countries: String) {
        require!(self.owner == env::predecessor_account_id(), "Unauthorized");
        let countries_split = countries.split("|");

        for code in countries_split {
            self.supported_countries.push(&code.to_string());
        }
    }

    pub fn set_min_players(&mut self, num: u8) {
        require!(self.owner == env::predecessor_account_id(), "Unauthorized");
        require!(num > 3, "Minimum number of players must be greater then 3");
        self.min_tournament_players = num;
    }

    pub fn set_percentage_stake_commission(&mut self, new_value: u64) {
        require!(self.owner == env::predecessor_account_id(), "Unauthorized");
        require!(new_value >= 1000, "Commission must be at least 10 percent");
        self.percentage_stake_commission = new_value;
    }

    // getters
    pub fn get_gametypes(&self, identifier: Option<String>) -> Vec<GameType> {

        if let Some(id) = Some(identifier) {
            let p = id.unwrap();
            return self.game_types.iter().filter(|game| game.identifier == p).collect::<Vec<GameType>>();
        } else {
            return self.game_types.iter().map(|game| game).collect::<Vec<GameType>>();
        }

    }

    pub fn get_tournament_by_key_or_name(&self, tournament_key: String, tournament_name: String) -> Option<Tournament> {
        let mut res = None;

        for tournament in self.tournaments.iter() {
            if tournament.tournament_key == tournament_key || tournament.name == tournament_name {
                res = Some(tournament);
            }
        }

        res
    }

    pub fn get_all_tournaments(&self, owner: Option<AccountId>) -> Vec<Tournament> {
        
        if let Some(owner) = Some(owner) {
            let p = owner.unwrap();
            return self.tournaments.iter().filter(|tournament| tournament.owner == p).collect::<Vec<Tournament>>();
        } else {
            return self.tournaments.iter().map(|tournament| tournament).collect::<Vec<Tournament>>();
        }
    }

    pub fn check_supported_country(&self, country_code: String) -> bool {
        for country in self.supported_countries.iter() {
            if country == country_code {
                return true
            }
        }
        false
    }

    pub fn get_position_prizes(&self) -> Vec<u64> {

        let percentage_to_pay: u64 = 10000 - self.percentage_stake_commission;
        let mut res = Vec::<u64>::with_capacity(3);

        res.push((5 * percentage_to_pay)/10);
        res.push((34 * percentage_to_pay)/100);
        res.push((16* percentage_to_pay)/100);

        res
    }
    
}

impl FungibleTokenReceiver for Wordchain {

    fn ft_on_transfer(&mut self,sender_id:AccountId,amount:U128,msg:String,) -> PromiseOrValue<U128> {
        let mut stakings = self.stakes.get(&sender_id).unwrap_or(LookupMap::new(b"g"));

        let mut stake_balance = stakings.get(&env::predecessor_account_id()).unwrap_or_default();

        stake_balance += amount.0;
        stakings.insert(&env::predecessor_account_id(), &stake_balance);
        self.stakes.insert(&sender_id, &stakings);

        PromiseOrValue::Value(U128::from(0))

        // let tournament_staked_for = msg.replace("tournament ", "");

        // let id = U128::try_from(tournament_staked_for);

    }
}


