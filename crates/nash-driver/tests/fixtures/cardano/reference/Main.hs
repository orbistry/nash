{-# LANGUAGE OverloadedStrings #-}
module Main where

import Codec.Serialise (serialise)
import qualified Data.ByteString.Lazy as LBS
import qualified PlutusLedgerApi.V3 as V
import qualified PlutusLedgerApi.V3.MintValue as Mint
import qualified PlutusLedgerApi.V1.Interval as Interval
import qualified PlutusTx as P
import qualified PlutusTx.AssocMap as M
import System.Directory (createDirectoryIfMissing)
import System.Environment (getArgs)

key = V.PubKeyCredential (V.PubKeyHash "key")
script = V.ScriptCredential (V.ScriptHash "script")
ref = V.TxOutRef (V.TxId "tx") 2
rep = V.DRep (V.DRepCredential key)
certificates =
  [ V.TxCertRegStaking key Nothing
  , V.TxCertUnRegStaking script (Just 12)
  , V.TxCertDelegStaking key (V.DelegStake (V.PubKeyHash "pool"))
  , V.TxCertRegDeleg key (V.DelegVote V.DRepAlwaysAbstain) 13
  , V.TxCertRegDRep (V.DRepCredential key) 14
  , V.TxCertUpdateDRep (V.DRepCredential script)
  , V.TxCertUnRegDRep (V.DRepCredential key) 15
  , V.TxCertPoolRegister (V.PubKeyHash "pool") (V.PubKeyHash "vrf")
  , V.TxCertPoolRetire (V.PubKeyHash "pool") 16
  , V.TxCertAuthHotCommittee (V.ColdCommitteeCredential key) (V.HotCommitteeCredential script)
  , V.TxCertResignColdCommittee (V.ColdCommitteeCredential script)
  , V.TxCertDelegStaking key (V.DelegStakeVote (V.PubKeyHash "pool") rep)
  , V.TxCertDelegStaking key (V.DelegVote V.DRepAlwaysNoConfidence)
  ]
actionId = V.GovernanceActionId (V.TxId "previous") 3
actions =
  [ V.ParameterChange Nothing (V.ChangedParameters (P.toBuiltinData (M.singleton (0 :: Integer) (42 :: Integer)))) (Just (V.ScriptHash "guard"))
  , V.HardForkInitiation (Just actionId) (V.ProtocolVersion 11 0)
  , V.TreasuryWithdrawals (M.unsafeFromList [(key, 17)]) Nothing
  , V.NoConfidence (Just actionId)
  , V.UpdateCommittee Nothing [V.ColdCommitteeCredential key] (M.unsafeFromList [(V.ColdCommitteeCredential script, 18)]) (V.unsafeRatio 2 3)
  , V.NewConstitution (Just actionId) (V.Constitution Nothing)
  , V.InfoAction
  , V.NewConstitution Nothing (V.Constitution (Just (V.ScriptHash "constitution")))
  ]
proposals = map (V.ProposalProcedure 19 key) actions
proposal = proposals !! 6
voters = [V.CommitteeVoter (V.HotCommitteeCredential key), V.DRepVoter (V.DRepCredential script), V.StakePoolVoter (V.PubKeyHash "pool")]
purposes = [V.Minting (V.CurrencySymbol "policy"), V.Spending ref, V.Rewarding key, V.Certifying 0 (head certificates), V.Voting (head voters), V.Proposing 6 proposal]
infos = [V.MintingScript (V.CurrencySymbol "policy"), V.SpendingScript ref (Just (V.Datum (P.toBuiltinData (99 :: Integer)))), V.RewardingScript key, V.CertifyingScript 0 (head certificates), V.VotingScript (head voters), V.ProposingScript 6 proposal]
coin = V.singleton (V.CurrencySymbol "") (V.TokenName "") 100
mint = Mint.UnsafeMintValue (M.unsafeFromList [(V.CurrencySymbol "policy", M.unsafeFromList [(V.TokenName "asset", -2)])])
addresses = [V.Address key Nothing, V.Address script (Just (V.StakingHash key)), V.Address key (Just (V.StakingPtr 20 21 22))]
outputs = zipWith (\address datum -> V.TxOut address coin datum (Just (V.ScriptHash "ref-script"))) addresses [V.NoOutputDatum, V.OutputDatumHash (V.DatumHash "datum"), V.OutputDatum (V.Datum (P.toBuiltinData (23 :: Integer)))]
tx = V.TxInfo
  [V.TxInInfo ref (head outputs)]
  [V.TxInInfo (V.TxOutRef (V.TxId "reference") 1) (last outputs)]
  outputs 2 mint certificates (M.unsafeFromList [(key, 24)])
  (V.Interval (V.LowerBound (V.Finite 10) False) (V.UpperBound (V.Finite 30) True))
  [V.PubKeyHash "key"]
  (M.unsafeFromList (zip purposes (repeat (V.Redeemer (P.toBuiltinData (25 :: Integer))))))
  (M.unsafeFromList [(V.DatumHash "datum", V.Datum (P.toBuiltinData (23 :: Integer)))])
  (V.TxId "transaction")
  (M.unsafeFromList (zipWith (\voter vote -> (voter, M.unsafeFromList [(actionId, vote)])) voters [V.VoteNo, V.VoteYes, V.Abstain]))
  proposals (Just 26) Nothing
contexts = map (V.ScriptContext tx redeemer) infos ++ [V.ScriptContext alternate redeemer (V.SpendingScript ref Nothing)]
  where
    redeemer = V.Redeemer (P.toBuiltinData (27 :: Integer))
    alternate = tx { V.txInfoCurrentTreasuryAmount = Nothing, V.txInfoTreasuryDonation = Just 28, V.txInfoOutputs = [V.TxOut (head addresses) coin V.NoOutputDatum Nothing] }

main = do
  [directory] <- getArgs
  createDirectoryIfMissing True directory
  mapM_ (\(index, context) -> LBS.writeFile (directory ++ "/context-" ++ show index ++ ".cbor") (serialise (P.toData context))) (zip [0 :: Int ..] contexts)
  let ranges = [V.Interval (V.LowerBound lo lc) (V.UpperBound hi hc) | lo <- [V.NegInf, V.Finite (-1), V.Finite 0, V.Finite 1, V.PosInf], hi <- [V.NegInf, V.Finite (-1), V.Finite 0, V.Finite 1, V.PosInf], lc <- [False, True], hc <- [False, True]] :: [V.POSIXTimeRange]
  LBS.writeFile (directory ++ "/intervals.cbor") (serialise (P.toData ranges))
  LBS.writeFile (directory ++ "/interval-results.cbor") (serialise (P.toData (map Interval.isEmpty ranges, [Interval.member n r | r <- ranges, n <- [-2..2]], [Interval.contains a b | a <- ranges, b <- ranges])))
  let v = V.singleton (V.CurrencySymbol "policy") (V.TokenName "asset") 5
      quantity x = V.valueOf x (V.CurrencySymbol "policy") (V.TokenName "asset")
      amounts = [quantity v, V.valueOf v (V.CurrencySymbol "policy") (V.TokenName "missing"), V.valueOf v (V.CurrencySymbol "missing") (V.TokenName "asset"), quantity (V.unionWith (+) v v), quantity (V.scale (-2) v), quantity (V.singleton (V.CurrencySymbol "policy") (V.TokenName "asset") 0)]
      comparisons = [V.geq (V.scale 2 v) v, V.geq v (V.scale 2 v), V.geq v (V.singleton (V.CurrencySymbol "policy") (V.TokenName "missing") 1)]
  LBS.writeFile (directory ++ "/values.cbor") (serialise (P.toData (amounts, comparisons)))
