
use near_contract_standards::fungible_token::metadata::{
    FungibleTokenMetadata, FungibleTokenMetadataProvider, FT_METADATA_SPEC,
};
use near_contract_standards::fungible_token::FungibleToken;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::{LazyOption, LookupMap, Vector};
use near_sdk::json_types::U128;
use near_sdk::{env, log, near_bindgen, AccountId, Balance, PanicOnDefault, BorshStorageKey, PromiseOrValue, ext_contract, require};

pub const DAY_TO_MS: u64 = 86400000;

#[derive(Eq, PartialEq)]
#[derive(Clone, BorshDeserialize, BorshSerialize)]
enum TournamentType {
    PUBLIC,
    PRIVATE,
    COUNTRY_BASED,
}

#[derive(Eq, PartialEq)]
#[derive(Clone, BorshDeserialize, BorshSerialize)]
enum TournamentState {
    PENDING_VOLUME,
    ACTIVE,
    DELETED,
    CLOSED
}

#[derive(Eq, PartialEq)]
#[derive(Clone, BorshDeserialize, BorshSerialize)]
enum PlayerState {
    ACTIVE,
    BLACKLISTED,
}

#[derive(Clone, BorshDeserialize, BorshSerialize)]
struct GameType {
    identifier: String,
    max_score: u64,
}

#[derive(Debug, Eq, Ord, PartialEq, PartialOrd)]
#[derive(Clone, BorshDeserialize, BorshSerialize)]
struct TournamentPlayer {
    id: U128,
    account_id: AccountId,
    stake_amount: Balance,
    score: u64,
    join_date: u64
}

#[derive(BorshDeserialize, BorshSerialize)]
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
    tournament_deadline: u64,
    tournament_type: TournamentType,
    status: TournamentState,
    players: Vector<TournamentPlayer>
}

#[ext_contract(ext_token_contract)]
pub trait StableCoin {
    fn internal_transfer(&self, from: &AccountId, to: &AccountId, amount: Balance, );

}

#[derive(BorshDeserialize, BorshSerialize)]
struct Wordchain {
    paused: bool,
    owner: AccountId,
    pending_owner: AccountId,
    token_address: String,
    min_tournament_players: u8,
    stake_payouts: Balance,
    percentage_stake_commission: u64,
    game_types: Vector<GameType>,
    tournaments_to_players: LookupMap<U128, Vector<AccountId>>,
    tournaments: Vector<Tournament>,
    supported_countries: Vector<String>,
}

impl Wordchain {

    /// .
    pub fn create_tournament(&mut self, 
        name: String, 
        tournament_key: String,
        game_type_id: String, 
        form: TournamentType, 
        interval: u64, 
        minimum_stake: U128, 
        country: Option<String>) -> Tournament {

        match form {
            TournamentType::COUNTRY_BASED => {
                require!(country.is_some(), "Country based tournament requires a country to be passed");
                require!(self.check_supported_country(country.clone().unwrap_or_default()), "Country code entered is not supported");
            },
            _ => {}
        }

        require!(self.get_tournament_by_key(tournament_key.clone()).is_none(), "Tournament with provided key already exists");
        require!(self.get_tournament_by_name(name.clone()).is_none(), "Tournament with provided name already exists");
        require!(self.get_gametypes(Some(game_type_id.clone())).len() > 0, "No tournament with provided game type");

        let tournament_id = U128::from((self.tournaments.len() as u128) + 1);

        let mut players_ = Vector::new(b"p");

        if env::predecessor_account_id() != self.owner {
            players_.push(&TournamentPlayer {
                id: U128::from(1),
                account_id: env::predecessor_account_id(),
                stake_amount: minimum_stake.0,
                score: 0,
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
            created_at: env::block_timestamp_ms(),
            tournament_deadline: env::block_timestamp_ms() + (interval * DAY_TO_MS),
            tournament_type: form,
            status: TournamentState::PENDING_VOLUME,
            players: players_
        };

        self.tournaments.push(&tournament);

        tournament

    }


    /// .
    pub fn join_tournament(&mut self,
        tournament_id: U128,
        stake_amount: U128,
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

        let number_of_players = (tournament.players.len() as u128) + 1;

        let player_id = U128::from(number_of_players.clone());

        tournament.players.push(&TournamentPlayer {
            id: player_id,
            account_id: env::predecessor_account_id(),
            stake_amount: stake_amount.0,
            score: 0,
            join_date: env::block_timestamp_ms(),
        });

        if number_of_players >= self.min_tournament_players as u128 {
            tournament.status = TournamentState::ACTIVE;
        }

        self.tournaments.replace(tournament_id.0 as u64, &tournament);
        
    }


    #[doc = r"Function to handle score update after playing a game"]
    pub fn publish_score(&mut self, tournament_id: U128, score: u8) {

        let mut tournament = self.tournaments.get(tournament_id.0 as u64).unwrap_or_else(|| env::panic_str("Tournament with provided ID does not exist"));
        require!(tournament.tournament_deadline > env::block_timestamp_ms(), "Tournament exceeded deadline");
        require!(tournament.status == TournamentState::ACTIVE, "Tournament can not be joined at this time");

        let gametype = self.get_gametypes(Some(tournament.game_type_id.clone()));
        require!(score as u64 <= gametype[0].max_score, "Score exceeds threshold for game");

        tournament.players.iter().for_each(|mut player| {
            if player.account_id == env::predecessor_account_id() {
                player.score += score as u64;
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

        match tournament.status {
            TournamentState::PENDING_VOLUME => {
                tournament.players.iter().for_each(|player| {
                    
                    if player.account_id != tournament.owner {

                    } else {
                        self.stake_payouts += player.stake_amount;
                    }
                });

                tournament.status = TournamentState::CLOSED;
                self.tournaments.replace(tournament_id.0 as u64, &tournament);
            },
            TournamentState::DELETED => {},
            _ => {
                let commission = (self.percentage_stake_commission as u128 * tournament.total_stake) / 10000;
                self.stake_payouts += commission;

                let pay_value = tournament.total_stake - commission;

                let mut players = tournament.players.iter().map(|player| player).collect::<Vec<TournamentPlayer>>();

                players.sort_by(|a, b| b.score.cmp(&a.score));

                let prizes = self.get_position_prizes();

                for i in 1..prizes.len() {
                    let val_to_pay = prizes[i] as u128 * pay_value;


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






    // getters
    pub fn get_gametypes(&self, identifier: Option<String>) -> Vec<GameType> {

        if let Some(id) = Some(identifier) {
            let p = id.unwrap();
            return self.game_types.iter().filter(|game| game.identifier == p).collect::<Vec<GameType>>();
        } else {
            return self.game_types.iter().map(|game| game).collect::<Vec<GameType>>();
        }

    }

    pub fn get_tournament_by_key(&self, tournament_key: String) -> Option<Tournament> {
        let mut res = None;

        for tournament in self.tournaments.iter() {
            if tournament.tournament_key == tournament_key {
                res = Some(tournament);
            }
        }

        res
    }

    pub fn get_tournament_by_name(&self, tournament_name: String) -> Option<Tournament> {
        let mut res = None;

        for tournament in self.tournaments.iter() {
            if tournament.name == tournament_name {
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


